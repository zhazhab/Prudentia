pub fn preview(request: PortfolioImportPreviewRequest) -> AppResult<PortfolioImportPreview> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    let headers = table.headers;
    let suggested_mapping = suggest_mapping(&headers);
    let mut validation_errors = validate_mapping(&headers, &suggested_mapping);

    if table.rows.is_empty() {
        validation_errors.push("file has no data rows".to_string());
    }

    let sample_rows = table
        .rows
        .iter()
        .take(8)
        .map(|row| row_to_map(&headers, row))
        .collect();
    let draft_rows = draft_rows_from_table(&headers, &table.rows, &suggested_mapping);

    Ok(PortfolioImportPreview {
        headers,
        sample_rows,
        suggested_mapping,
        validation_errors,
        draft_rows,
    })
}

pub fn draft_from_import(request: PortfolioImportDraftRequest) -> AppResult<PortfolioDraftPreview> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    let mut warnings = validate_mapping(&table.headers, &request.mapping);
    if table.rows.is_empty() {
        warnings.push("file has no data rows".to_string());
    }

    Ok(PortfolioDraftPreview {
        draft_rows: draft_rows_from_table(&table.headers, &table.rows, &request.mapping),
        warnings,
        source: "file".to_string(),
    })
}

pub async fn preview_image_import(
    ai: Arc<AiRuntime>,
    request: PortfolioImageImportPreviewRequest,
) -> AppResult<PortfolioImageImportPreview> {
    preview_image_import_with_progress(None, ai, request, |_| async {}).await
}

pub async fn preview_image_import_with_symbol_directory(
    pool: &SqlitePool,
    ai: Arc<AiRuntime>,
    request: PortfolioImageImportPreviewRequest,
) -> AppResult<PortfolioImageImportPreview> {
    preview_image_import_with_progress(Some(pool.clone()), ai, request, |_| async {}).await
}

pub async fn preview_image_import_with_progress<F, Fut>(
    symbol_pool: Option<SqlitePool>,
    ai: Arc<AiRuntime>,
    request: PortfolioImageImportPreviewRequest,
    mut progress: F,
) -> AppResult<PortfolioImageImportPreview>
where
    F: FnMut(&'static str) -> Fut,
    Fut: Future<Output = ()>,
{
    let started_at = Instant::now();
    tracing::info!(
        file_name = %request.file_name,
        mime_type = request.mime_type.as_deref().unwrap_or("unknown"),
        "portfolio image import preview started"
    );
    progress("validating_image").await;

    if !matches!(request.content_encoding.as_deref(), Some("base64")) {
        return Err(AppError::bad_request(
            "image imports must send content_encoding=base64",
        ));
    }

    let extension = supported_image_extension(&request.file_name, request.mime_type.as_deref())
        .ok_or_else(|| AppError::bad_request("unsupported image type"))?;
    let bytes = general_purpose::STANDARD.decode(request.content.trim())?;
    if bytes.is_empty() {
        return Err(AppError::bad_request("image content is empty"));
    }
    if bytes.len() > MAX_IMAGE_IMPORT_BYTES {
        return Err(AppError::bad_request("image content is too large"));
    }
    tracing::info!(
        file_name = %request.file_name,
        mime_type = request.mime_type.as_deref().unwrap_or("unknown"),
        extension,
        image_bytes = bytes.len(),
        "portfolio image import payload validated"
    );

    progress("writing_temp_image").await;
    let temp_image = TemporaryImportFile::write("prudentia-portfolio-image", extension, &bytes)?;
    progress("recognizing_image").await;
    let recognition_started_at = Instant::now();
    let recognition = ai
        .recognize_portfolio_image(&temp_image.path)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;
    tracing::info!(
        file_name = %request.file_name,
        elapsed_ms = recognition_started_at.elapsed().as_millis(),
        recognized_rows = recognition.rows.len(),
        recognition_warnings = recognition.warnings.len(),
        "portfolio image recognition provider returned"
    );
    progress("normalizing_rows").await;
    let mut warnings = recognition.warnings;
    if recognition.rows.is_empty() && warnings.is_empty() {
        warnings.push("No visible holding rows were recognized.".to_string());
    }

    let rows = clean_image_recognition_rows(recognition.rows);
    let mut draft_rows = draft_rows_from_image_recognition_rows(rows.clone());
    let resolved_symbol_count = if let Some(pool) = &symbol_pool {
        progress("resolving_symbols").await;
        resolve_missing_draft_symbols(pool, &mut draft_rows).await?
    } else {
        0
    };
    let row_count = draft_rows.len();
    let error_count = draft_rows.iter().map(|row| row.errors.len()).sum::<usize>();
    tracing::info!(
        file_name = %request.file_name,
        elapsed_ms = started_at.elapsed().as_millis(),
        row_count,
        resolved_symbol_count,
        warning_count = warnings.len(),
        error_count,
        "portfolio image import preview normalized"
    );

    Ok(PortfolioImageImportPreview {
        draft_rows,
        rows,
        warnings,
        source: "codex_cli".to_string(),
    })
}

pub async fn commit_import(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    request: PortfolioImportCommitRequest,
) -> AppResult<PortfolioImportResult> {
    let table = read_tabular_content(
        &request.file_name,
        &request.content,
        request.content_encoding,
    )?;
    validate_mapping(&table.headers, &request.mapping)
        .into_iter()
        .next()
        .map_or(Ok(()), |message| Err(AppError::bad_request(message)))?;

    let mut imported = Vec::new();
    let mut skipped_count = 0;

    for (index, row) in table.rows.iter().enumerate() {
        match position_from_row(&table.headers, row, &request.mapping) {
            Ok(position) => {
                upsert_position(pool, &position).await?;
                imported.push(position);
            }
            Err(error) => {
                skipped_count += 1;
                tracing::warn!(row = index + 2, error = ?error, "skipping invalid portfolio row");
            }
        }
    }

    recompute_weights_with_fx(pool, market_data.clone()).await?;
    record_current_position_baselines(pool, "import_commit").await?;
    record_portfolio_performance_snapshot(pool, market_data, "import_commit").await?;

    Ok(PortfolioImportResult {
        imported_count: imported.len(),
        skipped_count,
        positions: list_positions(pool).await?,
    })
}

pub async fn commit_draft_rows(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    request: PortfolioDraftCommitRequest,
) -> AppResult<PortfolioImportResult> {
    let resolver = LocalSymbolDirectoryResolver::new(pool);
    commit_draft_rows_with_symbol_resolver(pool, market_data, request, &resolver).await
}

async fn commit_draft_rows_with_symbol_resolver(
    pool: &SqlitePool,
    market_data: Arc<dyn MarketDataProvider>,
    request: PortfolioDraftCommitRequest,
    symbol_resolver: &dyn PortfolioSymbolResolver,
) -> AppResult<PortfolioImportResult> {
    if request.rows.is_empty() {
        return Err(AppError::bad_request("draft has no rows"));
    }

    let mut validation_errors = Vec::new();
    let mut positions_by_symbol: HashMap<String, PortfolioPosition> = HashMap::new();
    let mut position_symbols = Vec::new();
    let existing_positions = list_positions(pool).await?;
    for (index, row) in request.rows.into_iter().enumerate() {
        let mut row = normalize_draft_row(row);

        if row.symbol.is_empty() && !row.name.is_empty() {
            match resolve_symbol_from_existing_positions(
                &existing_positions,
                &row.name,
                &row.market,
                &row.currency,
            )
            .map(|matched| matched.symbol)
            {
                Some(symbol) => {
                    tracing::info!(
                        company_name = %row.name,
                        resolved_symbol = %symbol,
                        market = %row.market,
                        currency = %row.currency,
                        "portfolio draft symbol resolved from existing position"
                    );
                    apply_resolved_symbol_to_draft_row(&mut row, &symbol);
                }
                None => {
                    match symbol_resolver
                        .resolve_symbol(&row.name, &row.market, &row.currency)
                        .await?
                    {
                        Some(symbol) => {
                            tracing::info!(
                                company_name = %row.name,
                                resolved_symbol = %symbol,
                                market = %row.market,
                                currency = %row.currency,
                                "portfolio draft symbol resolved from local directory"
                            );
                            apply_resolved_symbol_to_draft_row(&mut row, &symbol);
                        }
                        None => {
                            validation_errors.push(format!(
                                "row {}: symbol could not be resolved for company name {}",
                                index + 1,
                                row.name
                            ));
                            continue;
                        }
                    }
                }
            }
        }

        let errors = validate_draft_row(&row);
        if errors.is_empty() {
            let position = position_from_draft_row(&row)?;
            if let Some(existing) = positions_by_symbol.get_mut(&position.symbol) {
                if let Err(error) = merge_duplicate_position(existing, position) {
                    validation_errors.push(format!("row {}: {}", index + 1, error));
                }
            } else {
                position_symbols.push(position.symbol.clone());
                positions_by_symbol.insert(position.symbol.clone(), position);
            }
        } else {
            validation_errors.push(format!("row {}: {}", index + 1, errors.join("; ")));
        }
    }

    if !validation_errors.is_empty() {
        return Err(AppError::bad_request(validation_errors.join(" ")));
    }

    let positions = position_symbols
        .iter()
        .filter_map(|symbol| positions_by_symbol.get(symbol))
        .collect::<Vec<_>>();

    for position in &positions {
        upsert_position(pool, position).await?;
    }

    recompute_weights_with_fx(pool, market_data.clone()).await?;
    record_current_position_baselines(pool, "draft_commit").await?;
    record_portfolio_performance_snapshot(pool, market_data, "draft_commit").await?;

    Ok(PortfolioImportResult {
        imported_count: positions.len(),
        skipped_count: 0,
        positions: list_positions(pool).await?,
    })
}

fn merge_duplicate_position(
    existing: &mut PortfolioPosition,
    next: PortfolioPosition,
) -> AppResult<()> {
    if existing.currency != next.currency {
        return Err(AppError::bad_request(format!(
            "duplicate symbol {} has conflicting currency",
            existing.symbol
        )));
    }
    if existing.market != next.market {
        return Err(AppError::bad_request(format!(
            "duplicate symbol {} has conflicting market",
            existing.symbol
        )));
    }

    let existing_cost = existing.average_cost * existing.quantity;
    let next_cost = next.average_cost * next.quantity;
    existing.quantity += next.quantity;
    existing.average_cost = ratio(existing_cost + next_cost, existing.quantity);
    existing.market_value += next.market_value;
    existing.unrealized_pnl = existing.market_value - existing.average_cost * existing.quantity;
    existing.last_price = Some(ratio(existing.market_value, existing.quantity));
    existing.account = merge_optional_position_text(existing.account.take(), next.account);
    existing.sector = merge_optional_position_text(existing.sector.take(), next.sector);
    existing.notes = merge_optional_position_text(existing.notes.take(), next.notes);
    if existing.name.trim().is_empty() {
        existing.name = next.name;
    }

    Ok(())
}

fn merge_optional_position_text(left: Option<String>, right: Option<String>) -> Option<String> {
    let mut values = Vec::new();
    for value in [left, right].into_iter().flatten() {
        let trimmed = value.trim();
        if !trimmed.is_empty() && !values.iter().any(|existing| existing == trimmed) {
            values.push(trimmed.to_string());
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}
