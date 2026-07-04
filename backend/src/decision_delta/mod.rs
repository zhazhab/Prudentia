use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::{
    decision::{self, Decision},
    error::{AppError, AppResult},
    investment_system::{self, InvestmentSystem, UpdateInvestmentSystemRequest},
    locale::Locale,
    market_data::{ExchangeRate, MarketDataProvider, MarketQuote},
    portfolio,
    state::AppState,
    time::now_iso,
};

const BASE_CURRENCY: &str = "CNY";
const DEFAULT_SNAPSHOT_LIMIT: usize = 90;
const MAX_SNAPSHOT_LIMIT: usize = 365;

include!("types.rs");
include!("routes.rs");
include!("legs.rs");
include!("refresh.rs");
include!("storage.rs");
include!("util.rs");
