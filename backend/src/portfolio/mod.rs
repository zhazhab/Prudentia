use std::{
    collections::{HashMap, HashSet},
    fs,
    future::Future,
    io::{Cursor, Read},
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use calamine::{Reader, Xlsx};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    ai::runtime::AiRuntime,
    error::{AppError, AppResult},
    market_data::{ExchangeRate, MarketDataProvider},
    state::AppState,
    time::now_iso,
};

const MAX_IMAGE_IMPORT_BYTES: usize = 10 * 1024 * 1024;
const BASE_CURRENCY: &str = "CNY";
const TUSHARE_API_URL: &str = "http://api.tushare.pro";

fn is_mock_market_data_source(source: &str) -> bool {
    source.trim().eq_ignore_ascii_case("mock")
}

include!("types.rs");
include!("performance.rs");
include!("performance_window.rs");
include!("performance_returns.rs");
include!("cash_flows.rs");
include!("trades.rs");
include!("routes.rs");
include!("import_workflows.rs");
include!("symbol_directory.rs");
include!("public_symbol_directory.rs");
include!("symbol_resolution.rs");
include!("positions.rs");
include!("position_fx.rs");
include!("draft_processing.rs");
include!("draft_image.rs");
include!("tests.rs");
