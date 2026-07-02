use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    ai::{
        prompt::{
            extract_json_object, investment_system_refinement_prompt, memo_extraction_prompt,
            portfolio_review_prompt, research_distillation_prompt, stock_snapshot_prompt,
        },
        AiError, AiProvider, InvestmentSystemRefinement, MemoExtraction, PortfolioImageRecognition,
        PortfolioReviewContext, ResearchAnalysis, ResearchSourceInput, StockSnapshotContext,
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

        let json = extract_json_object(content).ok_or_else(|| {
            AiError::Provider("AI response did not include a JSON object".to_string())
        })?;

        serde_json::from_str(json).map_err(|err| {
            AiError::Provider(format!(
                "failed to parse AI JSON response: {err}. response: {json}"
            ))
        })
    }
}

#[async_trait]
impl AiProvider for OpenAiCompatibleProvider {
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
