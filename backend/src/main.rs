use std::net::SocketAddr;

use std::sync::Arc;

use prudentia_backend::{
    ai::runtime::AiRuntime, config::AppConfig, database, market_data, portfolio, startup,
};
use sqlx::sqlite::SqlitePoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "prudentia_backend=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env();
    database::ensure_sqlite_file(&config.database_url)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    database::migrate(&pool).await?;

    let ai_provider = Arc::new(AiRuntime::from_config(&config));
    let market_provider = market_data::provider_from_config(&config);

    portfolio::start_price_refresh_job(
        pool.clone(),
        market_provider.clone(),
        config.price_refresh_interval_secs,
        config.price_refresh_ttl_secs,
    );
    if !config
        .symbol_directory_provider
        .trim()
        .eq_ignore_ascii_case("local")
    {
        portfolio::start_symbol_directory_refresh_job(
            pool.clone(),
            config.symbol_directory_provider.clone(),
            config.symbol_directory_refresh_interval_secs,
        );
    }

    let app = startup::build_router(pool, ai_provider, market_provider);
    let addr: SocketAddr = config.bind_addr.parse()?;
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("Prudentia backend listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
