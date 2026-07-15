use std::{collections::HashMap, time::Instant};

use serde::Deserialize;
use tokio::{sync::mpsc, task::JoinHandle};

use super::{
    sanitize_error, send_socket_event, AiWsServerMessage, SocketEvent,
    MAX_ACTIVE_TASKS_PER_CONNECTION,
};
use crate::{
    ai::{MemoChatContext, MemoChatHistoryMessage},
    locale::Locale,
    memo::Memo,
    memo_thread::{
        self, AppendAssistantMessageRequest, CreateMemoThreadMessageRequest, MemoThreadMessageRole,
        MemoThreadMessageStatus, MemoThreadSummary,
    },
    portfolio,
    state::AppState,
};

#[derive(Debug, Clone, Deserialize)]
pub struct MemoChatUserMessage {
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoChatContextHints {
    pub last_thread_id: Option<String>,
}

pub(super) struct MemoChatStartRequest {
    pub request_id: String,
    pub thread_id: Option<String>,
    pub client_thread_id: Option<String>,
    pub locale: Option<String>,
    pub message: MemoChatUserMessage,
}

struct MemoChatTaskRequest {
    request_id: String,
    state: AppState,
    thread_id: Option<String>,
    client_thread_id: Option<String>,
    locale: Locale,
    content: String,
    events_tx: mpsc::UnboundedSender<SocketEvent>,
    connection_id: u64,
}

pub(super) fn start(
    request: MemoChatStartRequest,
    state: AppState,
    events_tx: &mpsc::UnboundedSender<SocketEvent>,
    active_tasks: &mut HashMap<String, JoinHandle<()>>,
    connection_id: u64,
) {
    if active_tasks.contains_key(&request.request_id) {
        send_socket_event(
            events_tx,
            AiWsServerMessage::Failed {
                request_id: request.request_id,
                code: "duplicate_request".to_string(),
                error: "request_id is already active".to_string(),
                thread_id: None,
                duration_ms: None,
            },
        );
        return;
    }

    if active_tasks.len() >= MAX_ACTIVE_TASKS_PER_CONNECTION {
        send_socket_event(
            events_tx,
            AiWsServerMessage::Failed {
                request_id: request.request_id,
                code: "too_many_active_tasks".to_string(),
                error: "too many active AI tasks on this connection".to_string(),
                thread_id: None,
                duration_ms: None,
            },
        );
        return;
    }

    let request_id = request.request_id;
    let task = MemoChatTaskRequest {
        request_id: request_id.clone(),
        state,
        thread_id: request.thread_id,
        client_thread_id: request.client_thread_id,
        locale: request
            .locale
            .as_deref()
            .map(Locale::from_accept_language)
            .unwrap_or(Locale::En),
        content: request.message.content,
        events_tx: events_tx.clone(),
        connection_id,
    };
    let handle = tokio::spawn(async move {
        run_task(task).await;
    });
    active_tasks.insert(request_id, handle);
}

async fn run_task(request: MemoChatTaskRequest) {
    let started_at = Instant::now();
    let thread = match memo_thread::create_thread_with_user_message(
        &request.state.pool,
        CreateMemoThreadMessageRequest {
            thread_id: request.thread_id.clone(),
            client_thread_id: request.client_thread_id.clone(),
            content: request.content.clone(),
            locale: request.locale,
        },
    )
    .await
    {
        Ok(thread) => thread,
        Err(error) => {
            send_socket_event(
                &request.events_tx,
                AiWsServerMessage::Failed {
                    request_id: request.request_id.clone(),
                    code: "memo_chat_failed".to_string(),
                    error: sanitize_error(error.to_string()),
                    thread_id: None,
                    duration_ms: Some(started_at.elapsed().as_millis()),
                },
            );
            let _ = request
                .events_tx
                .send(SocketEvent::Finished(request.request_id));
            return;
        }
    };

    send_socket_event(
        &request.events_tx,
        AiWsServerMessage::Accepted {
            request_id: request.request_id.clone(),
            thread_id: Some(thread.id.clone()),
        },
    );

    if let Some(response) = casual_chat_response(&request.content, request.locale) {
        for chunk in response_chunks(&response) {
            send_socket_event(
                &request.events_tx,
                AiWsServerMessage::Delta {
                    request_id: request.request_id.clone(),
                    thread_id: thread.id.clone(),
                    content: chunk,
                },
            );
        }

        let assistant = memo_thread::append_assistant_message(
            &request.state.pool,
            &thread.id,
            AppendAssistantMessageRequest {
                request_id: Some(request.request_id.clone()),
                content: response,
                status: MemoThreadMessageStatus::Completed,
                duration_ms: Some(started_at.elapsed().as_millis() as i64),
                artifacts: Vec::new(),
                sources: Vec::new(),
                used_context: Vec::new(),
            },
        )
        .await;

        let (message_id, data) = match assistant {
            Ok(message) => (
                Some(message.id),
                serde_json::json!({ "thread_id": thread.id, "status": "completed" }),
            ),
            Err(error) => (
                None,
                serde_json::json!({ "persistence_error": sanitize_error(error.to_string()) }),
            ),
        };

        send_socket_event(
            &request.events_tx,
            AiWsServerMessage::Completed {
                request_id: request.request_id.clone(),
                artifact_type: "memo_chat.message".to_string(),
                data,
                thread_id: Some(thread.id),
                message_id,
                duration_ms: Some(started_at.elapsed().as_millis()),
            },
        );
        let _ = request
            .events_tx
            .send(SocketEvent::Finished(request.request_id));
        return;
    }

    if !should_create_memo_draft(&request.content) {
        let context = match memo_chat_context(&request.state, &thread, request.content.as_str())
            .await
        {
            Ok(context) => context,
            Err(error) => {
                send_memo_chat_failure(&request, Some(thread.id), started_at, error.to_string())
                    .await;
                return;
            }
        };

        let response = match request
            .state
            .ai
            .respond_to_memo_chat(&context, request.locale)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                send_memo_chat_failure(&request, Some(thread.id), started_at, error.to_string())
                    .await;
                return;
            }
        };

        for chunk in response_chunks(&response) {
            send_socket_event(
                &request.events_tx,
                AiWsServerMessage::Delta {
                    request_id: request.request_id.clone(),
                    thread_id: thread.id.clone(),
                    content: chunk,
                },
            );
        }

        let assistant = memo_thread::append_assistant_message(
            &request.state.pool,
            &thread.id,
            AppendAssistantMessageRequest {
                request_id: Some(request.request_id.clone()),
                content: response,
                status: MemoThreadMessageStatus::Completed,
                duration_ms: Some(started_at.elapsed().as_millis() as i64),
                artifacts: Vec::new(),
                sources: Vec::new(),
                used_context: Vec::new(),
            },
        )
        .await;

        let (message_id, data) = match assistant {
            Ok(message) => (
                Some(message.id),
                serde_json::json!({ "thread_id": thread.id, "status": "completed" }),
            ),
            Err(error) => (
                None,
                serde_json::json!({ "persistence_error": sanitize_error(error.to_string()) }),
            ),
        };

        send_socket_event(
            &request.events_tx,
            AiWsServerMessage::Completed {
                request_id: request.request_id.clone(),
                artifact_type: "memo_chat.message".to_string(),
                data,
                thread_id: Some(thread.id),
                message_id,
                duration_ms: Some(started_at.elapsed().as_millis()),
            },
        );
        let _ = request
            .events_tx
            .send(SocketEvent::Finished(request.request_id));
        return;
    }

    let memo_notes = memo_thread_notes(&request.state, &thread.id, &request.content)
        .await
        .unwrap_or_else(|_| request.content.clone());

    let memo = Memo {
        id: thread.id.clone(),
        title: thread.title.clone(),
        symbol: thread.linked_symbols.first().cloned(),
        asset_type: "stock".to_string(),
        thesis: String::new(),
        risks: String::new(),
        catalysts: String::new(),
        disconfirming_evidence: String::new(),
        notes: memo_notes,
        status: "draft".to_string(),
        tags: thread.tags.clone(),
        created_at: thread.created_at.clone(),
        updated_at: thread.updated_at.clone(),
    };

    let extraction = match request.state.ai.extract_memo(&memo, request.locale).await {
        Ok(extraction) => extraction,
        Err(error) => {
            let error = sanitize_error(error.to_string());
            let _ = memo_thread::append_assistant_message(
                &request.state.pool,
                &thread.id,
                AppendAssistantMessageRequest {
                    request_id: Some(request.request_id.clone()),
                    content: error.clone(),
                    status: MemoThreadMessageStatus::Failed,
                    duration_ms: Some(started_at.elapsed().as_millis() as i64),
                    artifacts: Vec::new(),
                    sources: Vec::new(),
                    used_context: Vec::new(),
                },
            )
            .await;
            send_socket_event(
                &request.events_tx,
                AiWsServerMessage::Failed {
                    request_id: request.request_id.clone(),
                    code: "memo_chat_failed".to_string(),
                    error,
                    thread_id: Some(thread.id.clone()),
                    duration_ms: Some(started_at.elapsed().as_millis()),
                },
            );
            let _ = request
                .events_tx
                .send(SocketEvent::Finished(request.request_id));
            return;
        }
    };

    let response = memo_chat_response(&extraction, request.locale);
    for chunk in response_chunks(&response) {
        tracing::debug!(
            request.connection_id,
            request_id = %request.request_id,
            thread_id = %thread.id,
            bytes = chunk.len(),
            "memo chat delta"
        );
        send_socket_event(
            &request.events_tx,
            AiWsServerMessage::Delta {
                request_id: request.request_id.clone(),
                thread_id: thread.id.clone(),
                content: chunk,
            },
        );
    }

    let draft = serde_json::json!({
        "core_hypothesis": extraction.thesis,
        "supporting_evidence": [],
        "key_risks": extraction.risks,
        "disconfirming_conditions": extraction.disconfirming_evidence,
        "catalysts_or_monitors": extraction.catalysts,
        "open_questions": extraction.checklist,
        "linked_symbols": thread.linked_symbols,
    });
    send_socket_event(
        &request.events_tx,
        AiWsServerMessage::Artifact {
            request_id: request.request_id.clone(),
            thread_id: thread.id.clone(),
            artifact_type: "memo_draft".to_string(),
            data: draft.clone(),
        },
    );

    let used_context = serde_json::json!({
        "items": [
            { "kind": "memo_thread", "label": thread.title },
            { "kind": "portfolio_summary", "label": "Current portfolio summary" }
        ]
    });
    send_socket_event(
        &request.events_tx,
        AiWsServerMessage::Artifact {
            request_id: request.request_id.clone(),
            thread_id: thread.id.clone(),
            artifact_type: "used_context".to_string(),
            data: used_context.clone(),
        },
    );

    let assistant = memo_thread::append_assistant_message(
        &request.state.pool,
        &thread.id,
        AppendAssistantMessageRequest {
            request_id: Some(request.request_id.clone()),
            content: response,
            status: MemoThreadMessageStatus::Completed,
            duration_ms: Some(started_at.elapsed().as_millis() as i64),
            artifacts: vec![draft],
            sources: Vec::new(),
            used_context: vec![used_context],
        },
    )
    .await;

    let (message_id, data) = match assistant {
        Ok(message) => (
            Some(message.id),
            serde_json::json!({ "thread_id": thread.id, "status": "completed" }),
        ),
        Err(error) => (
            None,
            serde_json::json!({ "persistence_error": sanitize_error(error.to_string()) }),
        ),
    };

    send_socket_event(
        &request.events_tx,
        AiWsServerMessage::Completed {
            request_id: request.request_id.clone(),
            artifact_type: "memo_chat.message".to_string(),
            data,
            thread_id: Some(thread.id),
            message_id,
            duration_ms: Some(started_at.elapsed().as_millis()),
        },
    );
    let _ = request
        .events_tx
        .send(SocketEvent::Finished(request.request_id));
}

fn memo_chat_response(extraction: &crate::ai::MemoExtraction, locale: Locale) -> String {
    if locale.is_zh() {
        format!(
            "我先把这段讨论整理成一份备忘录草稿。\n\n**核心假设**\n{}\n\n**主要风险**\n{}\n\n**催化/观察指标**\n{}\n\n**反证条件**\n{}\n\n**待验证问题**\n{}",
            extraction.thesis,
            extraction.risks,
            extraction.catalysts,
            extraction.disconfirming_evidence,
            extraction
                .checklist
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        format!(
            "I organized this discussion into a memo draft.\n\n**Core hypothesis**\n{}\n\n**Key risks**\n{}\n\n**Catalysts / monitors**\n{}\n\n**Disconfirming conditions**\n{}\n\n**Open questions**\n{}",
            extraction.thesis,
            extraction.risks,
            extraction.catalysts,
            extraction.disconfirming_evidence,
            extraction
                .checklist
                .iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

async fn memo_chat_context(
    state: &AppState,
    thread: &MemoThreadSummary,
    user_message: &str,
) -> crate::error::AppResult<MemoChatContext> {
    let detail = memo_thread::get_detail(&state.pool, &thread.id, 20, None).await?;
    let portfolio_positions = portfolio::list_positions(&state.pool).await?;
    let portfolio_summary = portfolio::summary(&state.pool).await?;
    let recent_messages = detail
        .messages
        .into_iter()
        .map(|message| MemoChatHistoryMessage {
            role: memo_thread_role_name(message.role).to_string(),
            content: message.content,
        })
        .collect();

    Ok(MemoChatContext {
        thread_title: thread.title.clone(),
        thread_summary: thread.summary.clone(),
        user_message: user_message.to_string(),
        recent_messages,
        portfolio_summary,
        portfolio_positions,
    })
}

async fn memo_thread_notes(
    state: &AppState,
    thread_id: &str,
    fallback: &str,
) -> crate::error::AppResult<String> {
    let detail = memo_thread::get_detail(&state.pool, thread_id, 30, None).await?;
    let notes = detail
        .messages
        .into_iter()
        .map(|message| {
            format!(
                "{}: {}",
                memo_thread_role_name(message.role),
                message.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if notes.trim().is_empty() {
        Ok(fallback.to_string())
    } else {
        Ok(notes)
    }
}

fn memo_thread_role_name(role: MemoThreadMessageRole) -> &'static str {
    match role {
        MemoThreadMessageRole::User => "user",
        MemoThreadMessageRole::Assistant => "assistant",
        MemoThreadMessageRole::System => "system",
    }
}

async fn send_memo_chat_failure(
    request: &MemoChatTaskRequest,
    thread_id: Option<String>,
    started_at: Instant,
    error: String,
) {
    let error = sanitize_error(error);
    if let Some(thread_id) = thread_id.as_deref() {
        let _ = memo_thread::append_assistant_message(
            &request.state.pool,
            thread_id,
            AppendAssistantMessageRequest {
                request_id: Some(request.request_id.clone()),
                content: error.clone(),
                status: MemoThreadMessageStatus::Failed,
                duration_ms: Some(started_at.elapsed().as_millis() as i64),
                artifacts: Vec::new(),
                sources: Vec::new(),
                used_context: Vec::new(),
            },
        )
        .await;
    }

    send_socket_event(
        &request.events_tx,
        AiWsServerMessage::Failed {
            request_id: request.request_id.clone(),
            code: "memo_chat_failed".to_string(),
            error,
            thread_id,
            duration_ms: Some(started_at.elapsed().as_millis()),
        },
    );
    let _ = request
        .events_tx
        .send(SocketEvent::Finished(request.request_id.clone()));
}

fn casual_chat_response(content: &str, locale: Locale) -> Option<String> {
    let normalized = normalized_lightweight_message(content);
    if normalized.is_empty() {
        return None;
    }

    if is_greeting_message(&normalized) {
        return Some(if locale.is_zh() {
            "你好，我在。你可以直接说一个公司、持仓、交易想法或想复盘的问题，我会按投资备忘录的方式帮你整理。".to_string()
        } else {
            "Hi, I am here. Tell me a company, position, trade idea, or review question, and I will organize it as an investment memo.".to_string()
        });
    }

    if is_thanks_message(&normalized) {
        return Some(if locale.is_zh() {
            "不客气。你可以继续补充公司、持仓或交易想法，我会接着整理。".to_string()
        } else {
            "You are welcome. Add a company, position, or trade idea when you are ready, and I will keep organizing it.".to_string()
        });
    }

    None
}

fn should_create_memo_draft(content: &str) -> bool {
    let normalized = normalized_lightweight_message(content);
    if normalized.is_empty() {
        return false;
    }

    if matches!(
        normalized.as_str(),
        "保存" | "保存一下" | "记录" | "记录一下" | "save" | "saveit"
    ) {
        return true;
    }

    let mentions_memo = normalized.contains("备忘录")
        || normalized.contains("memo")
        || normalized.contains("草稿")
        || normalized.contains("记录");
    let asks_to_materialize = normalized.contains("保存")
        || normalized.contains("整理")
        || normalized.contains("生成")
        || normalized.contains("沉淀")
        || normalized.contains("写入")
        || normalized.contains("转成")
        || normalized.contains("形成")
        || normalized.contains("record")
        || normalized.contains("save")
        || normalized.contains("draft");

    mentions_memo && asks_to_materialize
}

fn normalized_lightweight_message(content: &str) -> String {
    content
        .trim()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .trim_matches(is_lightweight_boundary_punctuation)
        .to_lowercase()
}

fn is_lightweight_boundary_punctuation(character: char) -> bool {
    matches!(
        character,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '~'
            | '。'
            | '，'
            | '！'
            | '？'
            | '；'
            | '：'
            | '、'
            | '～'
    )
}

fn is_greeting_message(normalized: &str) -> bool {
    matches!(
        normalized,
        "你好"
            | "您好"
            | "嗨"
            | "哈喽"
            | "在吗"
            | "在不在"
            | "早"
            | "早上好"
            | "下午好"
            | "晚上好"
            | "hi"
            | "hello"
            | "hey"
            | "hellothere"
    )
}

fn is_thanks_message(normalized: &str) -> bool {
    matches!(
        normalized,
        "谢谢" | "感谢" | "多谢" | "谢了" | "thanks" | "thankyou" | "thankyou!"
    )
}

fn response_chunks(response: &str) -> Vec<String> {
    response
        .split_inclusive("\n\n")
        .filter(|chunk| !chunk.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casual_chat_response_answers_zh_greeting_without_memo_draft() {
        let response = casual_chat_response(" 你好！ ", Locale::Zh).expect("casual response");

        assert!(response.contains("你好"));
        assert!(!response.contains("备忘录草稿"));
        assert!(!response.contains("核心假设"));
    }

    #[test]
    fn casual_chat_response_answers_en_greeting_without_memo_draft() {
        let response = casual_chat_response("hello", Locale::En).expect("casual response");

        assert!(response.contains("Hi"));
        assert!(!response.contains("memo draft"));
        assert!(!response.contains("Core hypothesis"));
    }

    #[test]
    fn casual_chat_response_leaves_investment_questions_for_memo_extraction() {
        assert!(casual_chat_response("复盘腾讯广告复苏假设", Locale::Zh).is_none());
        assert!(
            casual_chat_response("Review Tencent ad recovery assumptions", Locale::En).is_none()
        );
    }

    #[test]
    fn memo_draft_intent_requires_explicit_materialization() {
        assert!(!should_create_memo_draft("复盘腾讯广告复苏假设"));
        assert!(should_create_memo_draft("把这段整理成备忘录"));
        assert!(should_create_memo_draft("保存"));
        assert!(should_create_memo_draft("save this memo"));
    }
}
