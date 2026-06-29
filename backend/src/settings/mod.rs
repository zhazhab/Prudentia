use axum::{extract::State, routing::get, Json, Router};

use crate::{
    ai::runtime::{AiSettingsResponse, UpdateAiSettingsRequest},
    error::{AppError, AppResult},
    state::AppState,
};

pub fn routes() -> Router<AppState> {
    Router::new().route("/ai", get(get_ai_settings).patch(update_ai_settings))
}

async fn get_ai_settings(State(state): State<AppState>) -> AppResult<Json<AiSettingsResponse>> {
    Ok(Json(state.ai.settings_response()))
}

async fn update_ai_settings(
    State(state): State<AppState>,
    Json(request): Json<UpdateAiSettingsRequest>,
) -> AppResult<Json<AiSettingsResponse>> {
    let settings = state
        .ai
        .update(request)
        .map_err(|err| AppError::bad_request(err.to_string()))?;
    Ok(Json(settings))
}
