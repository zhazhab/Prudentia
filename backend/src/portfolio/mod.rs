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
    market_data::MarketDataProvider,
    state::AppState,
    time::now_iso,
};

const MAX_IMAGE_IMPORT_BYTES: usize = 10 * 1024 * 1024;
const BASE_CURRENCY: &str = "CNY";
const TUSHARE_API_URL: &str = "http://api.tushare.pro";

include!("types.rs");
include!("performance.rs");
include!("routes.rs");
include!("import_workflows.rs");
include!("symbol_directory.rs");
include!("public_symbol_directory.rs");
include!("symbol_resolution.rs");
include!("positions.rs");
include!("draft_processing.rs");
include!("draft_image.rs");
include!("tests.rs");
