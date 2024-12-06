use std::env;

use ::tracing::Instrument;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_dsql::auth_token::{AuthTokenGenerator, Config};
use chrono::NaiveDateTime;
use lambda_http::http::StatusCode;
use lambda_http::{
    run, service_fn,
    tracing::{self, instrument},
    Error, Request, Response,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Pool, Postgres};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Todo {
    id: String,
    name: Option<String>,
    description: Option<String>,
    created_at: Option<NaiveDateTime>,
    updated_at: Option<NaiveDateTime>,
}

#[instrument(name = "Handler")]
async fn function_handler(
    pool: &Pool<Postgres>,
    event: Request,
) -> Result<Response<String>, Error> {
    let query_span = tracing::info_span!("Query Todos");
    let mut return_body = json!("Error Fetching Rows").to_string();
    let mut status_code = StatusCode::OK;
    let rows = sqlx::query_as!(
        Todo,
        r#"
        select id, name, description, created_at, updated_at from Todos limit 10;
        "#,
    )
    .fetch_all(pool)
    .instrument(query_span)
    .await;

    match rows {
        Ok(v) => {
            return_body = serde_json::to_string(&v).unwrap();
        }
        Err(e) => {
            tracing::error!("Error saving entity: {}", e);
            status_code = StatusCode::BAD_REQUEST;
        }
    }

    let response = Response::builder()
        .status(status_code)
        .header("Content-Type", "application/json")
        .body(return_body)
        .map_err(Box::new)?;
    Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let tracer = opentelemetry_datadog::new_pipeline()
        .with_service_name("dsql-select")
        .with_agent_endpoint("http://127.0.0.1:8126")
        .with_api_version(opentelemetry_datadog::ApiVersion::Version05)
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_sampler(opentelemetry_sdk::trace::Sampler::AlwaysOn)
                .with_id_generator(opentelemetry_sdk::trace::RandomIdGenerator::default()),
        )
        .install_simple()
        .unwrap();
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = tracing_subscriber::fmt::layer().json().flatten_event(true);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .without_time();

    Registry::default()
        .with(fmt_layer)
        .with(telemetry_layer)
        .with(logger)
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let region = "us-east-1";
    let cluster_endpoint = env::var("CLUSTER_ENDPOINT").expect("CLUSTER_ENDPOINT required");
    // Generate auth token
    let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let signer = AuthTokenGenerator::new(
        Config::builder()
            .hostname(&cluster_endpoint)
            .region(Region::new(region))
            .build()
            .unwrap(),
    );
    let password_token = signer
        .db_connect_admin_auth_token(&sdk_config)
        .await
        .unwrap();

    // Setup connections
    let connection_options = PgConnectOptions::new()
        .host(cluster_endpoint.as_str())
        .port(5432)
        .database("postgres")
        .username("admin")
        .password(password_token.as_str())
        .ssl_mode(sqlx::postgres::PgSslMode::VerifyFull);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(connection_options.clone())
        .await?;
    let shared = &pool;
    //run(service_fn(function_handler)).await

    run(service_fn(move |event: Request| async move {
        function_handler(shared, event).await
    }))
    .await
}
