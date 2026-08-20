use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Clone)]
struct AppState {
    db: Option<PgPool>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load apps/server/.env if present. Absent in production, where the
    // environment supplies DATABASE_URL directly; never an error either way.
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pos_server=info,axum=info".into()),
        )
        .init();

    // DB is optional at startup so the server boots even without compose running.
    let db = match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(5).connect_lazy(&url)?),
        Err(_) => {
            tracing::warn!("DATABASE_URL not set — /health/db will report unavailable");
            None
        }
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/health/db", get(health_db))
        .with_state(AppState { db });

    let addr = "127.0.0.1:8080";
    tracing::info!("pos-server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "pos-server", "version": env!("CARGO_PKG_VERSION") }))
}

async fn health_db(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match &state.db {
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "db": "unconfigured" })),
        ),
        Some(pool) => match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await
        {
            Ok(_) => (StatusCode::OK, Json(json!({ "db": "ok" }))),
            Err(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "db": "error", "detail": e.to_string() })),
            ),
        },
    }
}
