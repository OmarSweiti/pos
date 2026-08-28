use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Clone)]
struct AppState {
    db: Option<PgPool>,
}

enum DbHealthOutcome {
    Unconfigured,
    Healthy,
    Failed(sqlx::Error),
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
    let outcome = match &state.db {
        None => DbHealthOutcome::Unconfigured,
        Some(pool) => match sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await
        {
            Ok(_) => DbHealthOutcome::Healthy,
            Err(error) => DbHealthOutcome::Failed(error),
        },
    };

    db_health_response(outcome)
}

fn db_health_response(outcome: DbHealthOutcome) -> (StatusCode, Json<Value>) {
    match outcome {
        DbHealthOutcome::Unconfigured => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "db": "unconfigured" })),
        ),
        DbHealthOutcome::Healthy => (StatusCode::OK, Json(json!({ "db": "ok" }))),
        DbHealthOutcome::Failed(error) => {
            // `sqlx::Error` Display and Debug payloads may contain a connection URL or database
            // data. Log only a reviewed static variant label; microstep 3.10.4 owns the later
            // liveness/readiness health-check design.
            tracing::warn!(
                sqlx_error_variant = sqlx_error_variant(&error),
                "database health check failed"
            );
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "db": "error" })),
            )
        }
    }
}

fn sqlx_error_variant(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::Configuration(_) => "configuration",
        sqlx::Error::Database(_) => "database",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::PoolTimedOut => "pool_timed_out",
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::WorkerCrashed => "worker_crashed",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serialized_response(outcome: DbHealthOutcome) -> (StatusCode, String) {
        let (status, Json(body)) = db_health_response(outcome);
        (status, body.to_string())
    }

    #[test]
    fn database_error_response_contains_no_underlying_error_text() {
        let source_marker = "database-health-source-marker";
        let error = sqlx::Error::Protocol(source_marker.to_owned());
        let error_text = error.to_string();

        let (status, body) = serialized_response(DbHealthOutcome::Failed(error));

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            !body.contains(&error_text),
            "response contained the underlying database error"
        );
        assert!(body == r#"{"db":"error"}"#, "error response shape changed");
    }

    #[test]
    fn password_bearing_database_url_is_absent_from_the_serialized_response() {
        let password = ["not-a-real", "-password"].concat();
        let host = ["health-db", ".invalid"].concat();
        let url = format!("postgres://health_user:{password}@{host}:5432/pos");
        let error = sqlx::Error::Io(std::io::Error::other(url.clone()));
        let error_text = error.to_string();

        assert!(
            error_text.contains(&url),
            "fixture did not carry its connection URL"
        );

        let (status, body) = serialized_response(DbHealthOutcome::Failed(error));

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            !body.contains(&url),
            "response contained the connection URL"
        );
        assert!(!body.contains(&password), "response contained the password");
        assert!(
            !body.contains(&host),
            "response contained the database host"
        );
        assert!(body == r#"{"db":"error"}"#, "error response shape changed");
    }

    #[tokio::test]
    async fn unconfigured_database_returns_service_unavailable() {
        let (status, Json(body)) = health_db(State(AppState { db: None })).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body.to_string(), r#"{"db":"unconfigured"}"#);
    }

    #[test]
    fn healthy_database_returns_ok() {
        let (status, body) = serialized_response(DbHealthOutcome::Healthy);

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"db":"ok"}"#);
    }
}
