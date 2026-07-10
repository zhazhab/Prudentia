use std::{fs, path::Path};

use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    time::now_iso,
};

use super::types::{
    CompanyView, CompanyViewChanges, CompanyViewPatch, CompanyViewSections, CompanyViewVersion,
};

pub async fn load_company_view(pool: &SqlitePool, symbol: &str) -> AppResult<Option<CompanyView>> {
    let row = sqlx::query(
        r#"SELECT symbol, company_name, current_version, current_content_json,
                  markdown_path, updated_at
        FROM company_views WHERE symbol = ?"#,
    )
    .bind(symbol.trim().to_ascii_uppercase())
    .fetch_optional(pool)
    .await?;
    row.map(company_view_from_row).transpose()
}

pub async fn apply_company_view_patch(
    pool: &SqlitePool,
    workspace_dir: &Path,
    mut patch: CompanyViewPatch,
    action_id: Option<&str>,
    provenance: Value,
) -> AppResult<CompanyView> {
    patch.symbol = patch.symbol.trim().to_ascii_uppercase();
    patch.company_name = patch.company_name.trim().to_string();
    if patch.symbol.is_empty() || patch.company_name.is_empty() {
        return Err(AppError::bad_request(
            "company symbol and name are required",
        ));
    }
    let current = load_company_view(pool, &patch.symbol).await?;
    let current_version = current
        .as_ref()
        .map(|view| view.current_version)
        .unwrap_or(0);
    if patch.base_version != current_version {
        return Err(AppError::bad_request(format!(
            "company view changed from version {} to {}; regenerate the proposal",
            patch.base_version, current_version
        )));
    }
    let content = merge_sections(
        current.map(|view| view.content).unwrap_or_default(),
        patch.changes,
    );
    let version = current_version + 1;
    let relative_current = format!("companies/{}/view.md", safe_segment(&patch.symbol));
    let relative_history = format!(
        "companies/{}/history/{version}.md",
        safe_segment(&patch.symbol)
    );
    let markdown = render_markdown(&patch.symbol, &patch.company_name, version, &content);
    write_atomic(&workspace_dir.join(&relative_history), &markdown)?;
    write_atomic(&workspace_dir.join(&relative_current), &markdown)?;

    let updated_at = now_iso();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO company_views (
            symbol, company_name, current_version, current_content_json, markdown_path, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(symbol) DO UPDATE SET
            company_name = excluded.company_name,
            current_version = excluded.current_version,
            current_content_json = excluded.current_content_json,
            markdown_path = excluded.markdown_path,
            updated_at = excluded.updated_at"#,
    )
    .bind(&patch.symbol)
    .bind(&patch.company_name)
    .bind(version)
    .bind(serde_json::to_string(&content)?)
    .bind(&relative_current)
    .bind(&updated_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO company_view_versions (
            id, symbol, version, content_json, action_id, provenance_json,
            markdown_path, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&patch.symbol)
    .bind(version)
    .bind(serde_json::to_string(&content)?)
    .bind(action_id)
    .bind(serde_json::to_string(&provenance)?)
    .bind(&relative_history)
    .bind(&updated_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(CompanyView {
        symbol: patch.symbol,
        company_name: patch.company_name,
        current_version: version,
        content,
        markdown_path: relative_current,
        updated_at,
    })
}

pub async fn list_company_view_versions(
    pool: &SqlitePool,
    symbol: &str,
) -> AppResult<Vec<CompanyViewVersion>> {
    let rows = sqlx::query(
        r#"SELECT symbol, version, content_json, action_id, provenance_json,
                  markdown_path, created_at
        FROM company_view_versions WHERE symbol = ? ORDER BY version DESC"#,
    )
    .bind(symbol.trim().to_ascii_uppercase())
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(company_view_version_from_row)
        .collect()
}

pub async fn rollback_company_view(
    pool: &SqlitePool,
    workspace_dir: &Path,
    symbol: &str,
    target_version: i64,
    expected_version: i64,
) -> AppResult<CompanyView> {
    let symbol = symbol.trim().to_ascii_uppercase();
    let current = load_company_view(pool, &symbol)
        .await?
        .ok_or_else(|| AppError::not_found("company view not found"))?;
    if current.current_version != expected_version {
        return Err(AppError::bad_request(format!(
            "company view changed from version {expected_version} to {}; reload history",
            current.current_version
        )));
    }
    let target = sqlx::query(
        r#"SELECT symbol, version, content_json, action_id, provenance_json,
                  markdown_path, created_at
        FROM company_view_versions WHERE symbol = ? AND version = ?"#,
    )
    .bind(&symbol)
    .bind(target_version)
    .fetch_optional(pool)
    .await?
    .map(company_view_version_from_row)
    .transpose()?
    .ok_or_else(|| AppError::not_found("company view version not found"))?;
    let content = target.content;
    apply_company_view_patch(
        pool,
        workspace_dir,
        CompanyViewPatch {
            symbol,
            company_name: current.company_name,
            base_version: current.current_version,
            changes: CompanyViewChanges {
                business_quality: Some(content.business_quality),
                moat: Some(content.moat),
                financials: Some(content.financials),
                valuation_expectations: Some(content.valuation_expectations),
                thesis: Some(content.thesis),
                risks: Some(content.risks),
                catalysts: Some(content.catalysts),
                disconfirming_evidence: Some(content.disconfirming_evidence),
                open_questions: Some(content.open_questions),
            },
        },
        None,
        serde_json::json!({ "rollback_from_version": target_version }),
    )
    .await
}

fn merge_sections(
    mut current: CompanyViewSections,
    changes: CompanyViewChanges,
) -> CompanyViewSections {
    merge_text(&mut current.business_quality, changes.business_quality);
    merge_text(&mut current.moat, changes.moat);
    merge_text(&mut current.financials, changes.financials);
    merge_text(
        &mut current.valuation_expectations,
        changes.valuation_expectations,
    );
    merge_text(&mut current.thesis, changes.thesis);
    merge_text(&mut current.risks, changes.risks);
    merge_text(&mut current.catalysts, changes.catalysts);
    merge_text(
        &mut current.disconfirming_evidence,
        changes.disconfirming_evidence,
    );
    if let Some(questions) = changes.open_questions {
        current.open_questions = questions
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
    }
    current
}

fn merge_text(target: &mut String, value: Option<String>) {
    if let Some(value) = value.map(|value| value.trim().to_string()) {
        *target = value;
    }
}

fn render_markdown(
    symbol: &str,
    company_name: &str,
    version: i64,
    content: &CompanyViewSections,
) -> String {
    format!(
        "# {company_name} ({symbol})\n\nVersion: {version}\n\n## Business Quality\n{}\n\n## Moat\n{}\n\n## Financials\n{}\n\n## Valuation Expectations\n{}\n\n## Thesis\n{}\n\n## Risks\n{}\n\n## Catalysts\n{}\n\n## Disconfirming Evidence\n{}\n\n## Open Questions\n{}\n",
        content.business_quality,
        content.moat,
        content.financials,
        content.valuation_expectations,
        content.thesis,
        content.risks,
        content.catalysts,
        content.disconfirming_evidence,
        content
            .open_questions
            .iter()
            .map(|question| format!("- {question}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn write_atomic(path: &Path, content: &str) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::internal("company view path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        AppError::internal(format!("failed to create company view directory: {error}"))
    })?;
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    fs::write(&temporary, content)
        .map_err(|error| AppError::internal(format!("failed to write company view: {error}")))?;
    fs::rename(&temporary, path)
        .map_err(|error| AppError::internal(format!("failed to replace company view: {error}")))?;
    Ok(())
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn company_view_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<CompanyView> {
    Ok(CompanyView {
        symbol: row.try_get("symbol")?,
        company_name: row.try_get("company_name")?,
        current_version: row.try_get("current_version")?,
        content: serde_json::from_str(&row.try_get::<String, _>("current_content_json")?)?,
        markdown_path: row.try_get("markdown_path")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn company_view_version_from_row(row: sqlx::sqlite::SqliteRow) -> AppResult<CompanyViewVersion> {
    Ok(CompanyViewVersion {
        symbol: row.try_get("symbol")?,
        version: row.try_get("version")?,
        content: serde_json::from_str(&row.try_get::<String, _>("content_json")?)?,
        action_id: row.try_get("action_id")?,
        provenance: serde_json::from_str(&row.try_get::<String, _>("provenance_json")?)?,
        markdown_path: row.try_get("markdown_path")?,
        created_at: row.try_get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn company_view_updates_are_versioned_and_written_as_markdown() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        database::migrate(&pool).await.expect("migrate");
        let workspace = tempfile::tempdir().expect("workspace");
        let view = apply_company_view_patch(
            &pool,
            workspace.path(),
            CompanyViewPatch {
                symbol: "0700.HK".to_string(),
                company_name: "Tencent".to_string(),
                base_version: 0,
                changes: CompanyViewChanges {
                    thesis: Some(
                        "Advertising recovery supports durable cash generation.".to_string(),
                    ),
                    risks: Some("Capital allocation may disappoint.".to_string()),
                    ..CompanyViewChanges::default()
                },
            },
            None,
            serde_json::json!({ "test": true }),
        )
        .await
        .expect("apply patch");

        assert_eq!(view.current_version, 1);
        assert!(workspace.path().join(&view.markdown_path).exists());
        assert_eq!(
            load_company_view(&pool, "0700.HK")
                .await
                .expect("load")
                .expect("view")
                .content
                .risks,
            "Capital allocation may disappoint."
        );

        let second = apply_company_view_patch(
            &pool,
            workspace.path(),
            CompanyViewPatch {
                symbol: "0700.HK".to_string(),
                company_name: "Tencent".to_string(),
                base_version: 1,
                changes: CompanyViewChanges {
                    risks: Some("Regulation may pressure returns.".to_string()),
                    ..CompanyViewChanges::default()
                },
            },
            None,
            serde_json::json!({ "test": true }),
        )
        .await
        .expect("second patch");
        assert_eq!(second.current_version, 2);

        let rolled_back = rollback_company_view(&pool, workspace.path(), "0700.HK", 1, 2)
            .await
            .expect("rollback");
        assert_eq!(rolled_back.current_version, 3);
        assert_eq!(
            rolled_back.content.risks,
            "Capital allocation may disappoint."
        );
        assert_eq!(
            list_company_view_versions(&pool, "0700.HK")
                .await
                .expect("history")
                .len(),
            3
        );
        assert!(
            rollback_company_view(&pool, workspace.path(), "0700.HK", 1, 2)
                .await
                .is_err()
        );
    }
}
