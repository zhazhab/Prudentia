use std::{io::Cursor, path::Path};

use base64::{engine::general_purpose, Engine as _};
use calamine::{Reader, Xlsx};
use reqwest::Client;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    ai::ConversationAttachmentContext,
    error::{AppError, AppResult},
    time::now_iso,
};

use super::types::{ConversationAttachment, UploadAttachmentRequest};

const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;

pub async fn save_attachment(
    pool: &SqlitePool,
    workspace_dir: &Path,
    request: UploadAttachmentRequest,
) -> AppResult<ConversationAttachment> {
    let (file_name, mime_type, bytes, source_url) = if let Some(url) = clean(request.url) {
        fetch_url(&url).await?
    } else {
        let file_name = clean(request.file_name)
            .ok_or_else(|| AppError::bad_request("attachment file_name is required"))?;
        let mime_type = clean(request.mime_type).unwrap_or_else(|| infer_mime(&file_name));
        let content = request
            .content
            .ok_or_else(|| AppError::bad_request("attachment content is required"))?;
        let bytes = if request.content_encoding.as_deref() == Some("base64") {
            general_purpose::STANDARD.decode(content.trim())?
        } else {
            content.into_bytes()
        };
        (file_name, mime_type, bytes, None)
    };
    if bytes.is_empty() {
        return Err(AppError::bad_request("attachment is empty"));
    }
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::bad_request("attachment exceeds the 20 MB limit"));
    }

    let content_hash = hash_bytes(&bytes);
    if let Some(existing) = find_by_hash(pool, &content_hash).await? {
        return Ok(existing);
    }
    let safe_name = safe_file_name(&file_name);
    let relative_path = format!("attachments/{content_hash}/{safe_name}");
    let absolute_path = workspace_dir.join(&relative_path);
    std::fs::create_dir_all(
        absolute_path
            .parent()
            .ok_or_else(|| AppError::internal("attachment path has no parent"))?,
    )
    .map_err(|error| {
        AppError::internal(format!("failed to create attachment directory: {error}"))
    })?;
    std::fs::write(&absolute_path, &bytes)
        .map_err(|error| AppError::internal(format!("failed to save attachment: {error}")))?;

    let (extracted_text, parse_status, parse_error) =
        parse_attachment(&file_name, &mime_type, &bytes);
    let attachment = ConversationAttachment {
        id: Uuid::new_v4().to_string(),
        content_hash,
        file_name,
        mime_type,
        relative_path: Some(relative_path),
        source_url,
        extracted_text,
        parse_status,
        parse_error,
        size_bytes: bytes.len() as i64,
        created_at: now_iso(),
    };
    sqlx::query(
        r#"INSERT INTO conversation_attachments (
            id, content_hash, file_name, mime_type, relative_path, source_url,
            extracted_text, parse_status, parse_error, size_bytes, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&attachment.id)
    .bind(&attachment.content_hash)
    .bind(&attachment.file_name)
    .bind(&attachment.mime_type)
    .bind(&attachment.relative_path)
    .bind(&attachment.source_url)
    .bind(&attachment.extracted_text)
    .bind(&attachment.parse_status)
    .bind(&attachment.parse_error)
    .bind(attachment.size_bytes)
    .bind(&attachment.created_at)
    .execute(pool)
    .await?;
    Ok(attachment)
}

pub async fn load_attachment_contexts(
    pool: &SqlitePool,
    workspace_dir: &Path,
    run_id: &str,
) -> AppResult<Vec<ConversationAttachmentContext>> {
    let rows = sqlx::query(
        r#"SELECT attachment.id, attachment.file_name, attachment.mime_type,
                  attachment.relative_path, attachment.extracted_text, attachment.parse_status
        FROM conversation_attachments attachment
        JOIN conversation_run_attachments link ON link.attachment_id = attachment.id
        WHERE link.run_id = ? ORDER BY attachment.created_at ASC"#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let relative_path: Option<String> = row.try_get("relative_path")?;
            Ok(ConversationAttachmentContext {
                id: row.try_get("id")?,
                file_name: row.try_get("file_name")?,
                mime_type: row.try_get("mime_type")?,
                extracted_text: row.try_get("extracted_text")?,
                parse_status: row.try_get("parse_status")?,
                local_path: relative_path
                    .map(|relative_path| workspace_dir.join(relative_path).display().to_string()),
            })
        })
        .collect()
}

async fn fetch_url(url: &str) -> AppResult<(String, String, Vec<u8>, Option<String>)> {
    let response = Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|error| AppError::bad_request(format!("failed to fetch attachment URL: {error}")))?
        .error_for_status()
        .map_err(|error| {
            AppError::bad_request(format!("failed to fetch attachment URL: {error}"))
        })?;
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .unwrap_or("text/html")
        .to_string();
    let file_name = url
        .split('/')
        .next_back()
        .and_then(|segment| segment.split('?').next())
        .filter(|segment| !segment.is_empty())
        .unwrap_or("linked-source.html")
        .to_string();
    let bytes = response.bytes().await.map_err(|error| {
        AppError::bad_request(format!("failed to read attachment URL: {error}"))
    })?;
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::bad_request(
            "linked attachment exceeds the 20 MB limit",
        ));
    }
    Ok((file_name, mime_type, bytes.to_vec(), Some(url.to_string())))
}

fn parse_attachment(
    file_name: &str,
    mime_type: &str,
    bytes: &[u8],
) -> (Option<String>, String, Option<String>) {
    let extension = file_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if mime_type.starts_with("text/") || matches!(extension.as_str(), "txt" | "md" | "csv" | "tsv")
    {
        return match String::from_utf8(bytes.to_vec()) {
            Ok(text) => (Some(limit_text(text)), "parsed".to_string(), None),
            Err(error) => (None, "failed".to_string(), Some(error.to_string())),
        };
    }
    if extension == "pdf" || mime_type == "application/pdf" {
        return match pdf_extract::extract_text_from_mem(bytes) {
            Ok(text) => (Some(limit_text(text)), "parsed".to_string(), None),
            Err(error) => (None, "failed".to_string(), Some(error.to_string())),
        };
    }
    if extension == "xlsx" {
        return match extract_xlsx(bytes) {
            Ok(text) => (Some(limit_text(text)), "parsed".to_string(), None),
            Err(error) => (None, "failed".to_string(), Some(error)),
        };
    }
    if mime_type.starts_with("image/") {
        return (None, "ready".to_string(), None);
    }
    (
        None,
        "stored".to_string(),
        Some("file type is stored but has no text extractor".to_string()),
    )
}

fn extract_xlsx(bytes: &[u8]) -> Result<String, String> {
    let mut workbook = Xlsx::new(Cursor::new(bytes.to_vec())).map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    for sheet in workbook.sheet_names().to_vec() {
        let range = workbook
            .worksheet_range(&sheet)
            .map_err(|error| error.to_string())?;
        lines.push(format!("# {sheet}"));
        for row in range.rows() {
            lines.push(
                row.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\t"),
            );
        }
    }
    Ok(lines.join("\n"))
}

async fn find_by_hash(pool: &SqlitePool, hash: &str) -> AppResult<Option<ConversationAttachment>> {
    let row = sqlx::query(
        r#"SELECT id, content_hash, file_name, mime_type, relative_path, source_url,
                  extracted_text, parse_status, parse_error, size_bytes, created_at
        FROM conversation_attachments WHERE content_hash = ? ORDER BY created_at ASC LIMIT 1"#,
    )
    .bind(hash)
    .fetch_optional(pool)
    .await?;
    row.map(attachment_from_row).transpose()
}

fn attachment_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<ConversationAttachment> {
    Ok(ConversationAttachment {
        id: row.try_get("id")?,
        content_hash: row.try_get("content_hash")?,
        file_name: row.try_get("file_name")?,
        mime_type: row.try_get("mime_type")?,
        relative_path: row.try_get("relative_path")?,
        source_url: row.try_get("source_url")?,
        extracted_text: row.try_get("extracted_text")?,
        parse_status: row.try_get("parse_status")?,
        parse_error: row.try_get("parse_error")?,
        size_bytes: row.try_get("size_bytes")?,
        created_at: row.try_get("created_at")?,
    })
}

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn safe_file_name(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "attachment".to_string()
    } else {
        value
    }
}

fn infer_mime(file_name: &str) -> String {
    match file_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn limit_text(mut value: String) -> String {
    const MAX_CHARS: usize = 80_000;
    if value.chars().count() > MAX_CHARS {
        value = value.chars().take(MAX_CHARS).collect();
        value.push_str("\n[truncated]");
    }
    value
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn text_attachments_are_hash_deduplicated_and_extracted() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        database::migrate(&pool).await.expect("migrate");
        let workspace = tempfile::tempdir().expect("workspace");
        let request = UploadAttachmentRequest {
            file_name: Some("notes.md".to_string()),
            mime_type: Some("text/markdown".to_string()),
            content: Some(general_purpose::STANDARD.encode("# Thesis\nEvidence")),
            content_encoding: Some("base64".to_string()),
            url: None,
        };
        let first = save_attachment(&pool, workspace.path(), request.clone())
            .await
            .expect("save");
        let second = save_attachment(&pool, workspace.path(), request)
            .await
            .expect("deduplicate");

        assert_eq!(first.id, second.id);
        assert_eq!(first.parse_status, "parsed");
        assert_eq!(first.extracted_text.as_deref(), Some("# Thesis\nEvidence"));
        assert!(workspace
            .path()
            .join(first.relative_path.expect("path"))
            .exists());
    }
}
