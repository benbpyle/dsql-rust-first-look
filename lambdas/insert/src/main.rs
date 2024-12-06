use std::env;

use aws_config::{BehaviorVersion, Region};
use aws_sdk_dsql::auth_token::{AuthTokenGenerator, Config};
use lambda_http::http::StatusCode;
use lambda_http::{
    run, service_fn, tracing::{self, instrument}, Error, Request, RequestPayloadExt, Response

};
use ::tracing::{info, Instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Registry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoCreate {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    id: String,
    name: String,
    description: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<TodoCreate> for Todo {
    fn from(value: TodoCreate) -> Self {
        Todo {
            id: Uuid::new_v4().to_string(),
            description: value.description,
            name: value.name,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[instrument(name = "Handler")]
async fn function_handler(
    pool: &Pool<Postgres>,
    event: Request,
) -> Result<Response<String>, Error> {
    let body = event.payload::<TodoCreate>()?;
    let mut return_body = json!("").to_string();
    let mut status_code = StatusCode::OK;

    match body {
        Some(v) => {
            let e: Todo = v.into();
            let query_span =
                tracing::info_span!("Save Todo");
            let result = sqlx::query("INSERT INTO Todos (id, name, description, created_at, update_at) VALUES ($1, $2, $3, $4, $5)")
                .bind(e.id.clone())
                .bind(e.name.clone())
                .bind(e.description.clone())
                .bind(e.created_at) 
                .bind(e.updated_at)
                .execute(pool)
                .instrument(query_span)
                .await;

            match result {
                Ok(_) => {
                    let o = e.clone();
                    info!("(Todo)={:?}", o);
                    return_body = serde_json::to_string(&o).unwrap();
                }
                Err(e) => {
                    tracing::error!("Error saving entity: {}", e);
                    status_code = StatusCode::BAD_REQUEST;
                    return_body = serde_json::to_string("Error saving entity").unwrap()
                }
            }
        }
        None => {
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
        .with_service_name("dsql-insert")
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
