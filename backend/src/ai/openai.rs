use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::mpsc;

use crate::{
    ai::{
        prompt::{
            agent_execution_prompt, capability_execution_prompt, conversation_projection_prompt,
            conversation_response_prompt, investment_system_refinement_prompt, memo_chat_prompt,
            memo_extraction_prompt, parse_json_object, portfolio_review_prompt,
            research_distillation_prompt, stock_snapshot_prompt,
        },
        AgentModelRequest, AiError, AiProvider, AiProviderEvent, CapabilityModelRequest,
        ConversationContext, ConversationProjection, InvestmentSystemRefinement, MemoChatContext,
        MemoExtraction, PortfolioImageRecognition, PortfolioReviewContext, ResearchAnalysis,
        ResearchSourceInput, StockSnapshotContext,
    },
    investment_system::InvestmentSystem,
    locale::Locale,
    memo::Memo,
};

pub struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        }
    }

    async fn chat_text(&self, prompt: String) -> Result<String, AiError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: "You are Prudentia, a natural investment memo chat assistant.",
                },
                ChatMessage {
                    role: "user",
                    content: &prompt,
                },
            ],
            temperature: 0.4,
            stream: None,
        };

        let response: ChatResponse = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?
            .error_for_status()
            .map_err(|err| AiError::Provider(err.to_string()))?
            .json()
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?;

        response
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| AiError::Provider("AI response had no text".to_string()))
    }

    async fn chat_json<T: for<'de> Deserialize<'de>>(&self, prompt: String) -> Result<T, AiError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content:
                        "You are Prudentia, an investment memo assistant. Return strict JSON only.",
                },
                ChatMessage {
                    role: "user",
                    content: &prompt,
                },
            ],
            temperature: 0.2,
            stream: None,
        };

        let response: ChatResponse = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?
            .error_for_status()
            .map_err(|err| AiError::Provider(err.to_string()))?
            .json()
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?;

        let content = response
            .choices
            .first()
            .ok_or_else(|| AiError::Provider("AI response had no choices".to_string()))?
            .message
            .content
            .trim();

        parse_json_object(content)
            .map_err(|err| AiError::Provider(format!("failed to parse AI JSON response: {err}")))
    }

    async fn chat_stream(
        &self,
        prompt: String,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: "You are Prudentia, a natural investment memo chat assistant.",
                },
                ChatMessage {
                    role: "user",
                    content: &prompt,
                },
            ],
            temperature: 0.4,
            stream: Some(true),
        };
        let _ = events.send(AiProviderEvent::Stage {
            provider: "openai".to_string(),
            stage: "request_started".to_string(),
        });
        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(|err| AiError::Provider(err.to_string()))?
            .error_for_status()
            .map_err(|err| AiError::Provider(err.to_string()))?;
        let mut stream = response.bytes_stream();
        let mut pending = String::new();
        let mut complete = String::new();
        let mut writing_started = false;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|err| AiError::Provider(err.to_string()))?;
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(index) = pending.find('\n') {
                let line = pending[..index].trim().to_string();
                pending.drain(..=index);
                if let Some(delta) = parse_sse_delta(&line)? {
                    emit_stream_delta(&events, &mut complete, &mut writing_started, delta);
                }
            }
        }
        if let Some(delta) = parse_sse_delta(pending.trim())? {
            emit_stream_delta(&events, &mut complete, &mut writing_started, delta);
        }
        if complete.trim().is_empty() {
            return Err(AiError::Provider("AI response had no text".to_string()));
        }
        Ok(complete)
    }
}

#[async_trait]
impl AiProvider for OpenAiCompatibleProvider {
    async fn execute_capability(
        &self,
        request: &CapabilityModelRequest,
        locale: Locale,
    ) -> Result<serde_json::Value, AiError> {
        self.chat_json(capability_execution_prompt(request, locale))
            .await
    }

    async fn execute_agent_turn(
        &self,
        request: &AgentModelRequest,
        locale: Locale,
    ) -> Result<serde_json::Value, AiError> {
        self.chat_json(agent_execution_prompt(request, locale))
            .await
    }

    async fn respond_to_conversation(
        &self,
        context: &ConversationContext,
        locale: Locale,
        events: mpsc::UnboundedSender<AiProviderEvent>,
    ) -> Result<String, AiError> {
        self.chat_stream(conversation_response_prompt(context, locale), events)
            .await
    }

    async fn project_conversation(
        &self,
        context: &ConversationContext,
        assistant_response: &str,
        locale: Locale,
    ) -> Result<ConversationProjection, AiError> {
        self.chat_json(conversation_projection_prompt(
            context,
            assistant_response,
            locale,
        ))
        .await
    }

    async fn respond_to_memo_chat(
        &self,
        context: &MemoChatContext,
        locale: Locale,
    ) -> Result<String, AiError> {
        self.chat_text(memo_chat_prompt(context, locale)).await
    }

    async fn extract_memo(&self, memo: &Memo, locale: Locale) -> Result<MemoExtraction, AiError> {
        self.chat_json(memo_extraction_prompt(memo, locale)).await
    }

    async fn refine_system(
        &self,
        system: &InvestmentSystem,
        locale: Locale,
    ) -> Result<InvestmentSystemRefinement, AiError> {
        self.chat_json(investment_system_refinement_prompt(system, locale))
            .await
    }

    async fn distill_research_source(
        &self,
        input: &ResearchSourceInput,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.chat_json(research_distillation_prompt(input, locale))
            .await
    }

    async fn analyze_stock_snapshot(
        &self,
        context: &StockSnapshotContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.chat_json(stock_snapshot_prompt(context, locale)).await
    }

    async fn review_portfolio_risk(
        &self,
        context: &PortfolioReviewContext,
        locale: Locale,
    ) -> Result<ResearchAnalysis, AiError> {
        self.chat_json(portfolio_review_prompt(context, locale))
            .await
    }

    async fn recognize_portfolio_image(
        &self,
        _image_path: &Path,
    ) -> Result<PortfolioImageRecognition, AiError> {
        Err(AiError::Provider(
            "portfolio image recognition requires the CLI provider with image support".to_string(),
        ))
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: String,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: AssistantMessage,
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: String,
}

#[derive(Deserialize)]
struct StreamChatResponse {
    #[serde(default)]
    choices: Vec<StreamChatChoice>,
}

#[derive(Deserialize)]
struct StreamChatChoice {
    delta: StreamChatDelta,
}

#[derive(Deserialize)]
struct StreamChatDelta {
    content: Option<String>,
}

fn parse_sse_delta(line: &str) -> Result<Option<String>, AiError> {
    let Some(payload) = line.strip_prefix("data:").map(str::trim) else {
        return Ok(None);
    };
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(None);
    }
    let event: StreamChatResponse = serde_json::from_str(payload)
        .map_err(|err| AiError::Provider(format!("failed to parse AI stream event: {err}")))?;
    Ok(event
        .choices
        .into_iter()
        .find_map(|choice| choice.delta.content)
        .filter(|content| !content.is_empty()))
}

fn emit_stream_delta(
    events: &mpsc::UnboundedSender<AiProviderEvent>,
    complete: &mut String,
    writing_started: &mut bool,
    delta: String,
) {
    if !*writing_started {
        let _ = events.send(AiProviderEvent::Stage {
            provider: "openai".to_string(),
            stage: "provider_writing_response".to_string(),
        });
        *writing_started = true;
    }
    complete.push_str(&delta);
    let _ = events.send(AiProviderEvent::TextDelta(delta));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sse_delta_emits_a_truthful_writing_stage_once() {
        let (events, mut receiver) = mpsc::unbounded_channel();
        let mut complete = String::new();
        let mut writing_started = false;

        emit_stream_delta(
            &events,
            &mut complete,
            &mut writing_started,
            "hello".to_string(),
        );
        emit_stream_delta(
            &events,
            &mut complete,
            &mut writing_started,
            " world".to_string(),
        );

        assert_eq!(
            receiver.try_recv(),
            Ok(AiProviderEvent::Stage {
                provider: "openai".to_string(),
                stage: "provider_writing_response".to_string(),
            })
        );
        assert_eq!(
            receiver.try_recv(),
            Ok(AiProviderEvent::TextDelta("hello".to_string()))
        );
        assert_eq!(
            receiver.try_recv(),
            Ok(AiProviderEvent::TextDelta(" world".to_string()))
        );
        assert!(receiver.try_recv().is_err());
        assert_eq!(complete, "hello world");
    }
}
