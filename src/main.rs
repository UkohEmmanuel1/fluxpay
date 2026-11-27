use axum::{
    extract::{Path, State},
    http::{StatusCode, Request, HeaderMap},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use tracing::{info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    api_key: String,
    webhook_url: Option<String>,
    webhook_secret: String,
    receiving_address: Option<String>,
    network: String,
    dev_confirmation: bool,
    http: reqwest::Client,
}


#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
struct Payment {
    id: String,
    amount: String,
    currency: String,
    network: String,
    reference: Option<String>,
    status: String,
    payment_address: Option<String>,
    transaction_hash: Option<String>,
    created_at: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct CreatePayment {
    amount: String,
    currency: String,
    #[serde(default)]
    network: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DevConfirm {
    transaction_hash: String,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ApiError { error: message.into() })).into_response()
}

async fn auth(State(state): State<Arc<AppState>>, request: Request<axum::body::Body>, next: Next) -> Response {
    let expected = format!("Bearer {}", state.api_key);
    let valid = request.headers().get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok()).map(|value| value == expected).unwrap_or(false);
    if valid { next.run(request).await } else { error(StatusCode::UNAUTHORIZED, "Missing or invalid API key") }
}


async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status":"ok","service":"fluxpay","version":"0.2.0"}))
}


async fn readiness(State(state): State<Arc<AppState>>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => Json(serde_json::json!({"status":"ready","database":"ok"})).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"not_ready","database":"unavailable"}))).into_response(),
    }
}


async fn create_payment(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(input): Json<CreatePayment>) -> Response {
    // Parse decimal strings without binary floating-point rounding. Enforce a bounded scale.
    let amount = match input.amount.trim().parse::<rust_decimal::Decimal>() {
        Ok(v) if v.is_sign_positive() && !v.is_zero() && v.scale() <= 6 => v.normalize().to_string(),
        _ => return error(StatusCode::BAD_REQUEST, "amount must be a positive decimal with at most 6 fractional digits"),
    };
    if amount.len() > 32 { return error(StatusCode::BAD_REQUEST, "amount is too large"); }
    // Idempotency is persisted so client retries cannot create duplicate payment sessions.
    let idempotency_key = headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    if let Some(key) = &idempotency_key {
        if key.len() > 128 { return error(StatusCode::BAD_REQUEST, "idempotency-key must be 128 characters or fewer"); }
        if let Ok(Some(existing_id)) = sqlx::query_scalar::<_, String>("SELECT payment_id FROM idempotency_keys WHERE key = ?").bind(key).fetch_optional(&state.db).await {
            if let Ok(Some(existing)) = sqlx::query_as::<_, Payment>("SELECT id, amount, currency, network, reference, status, payment_address, transaction_hash, created_at, expires_at FROM payments WHERE id = ?").bind(existing_id).fetch_optional(&state.db).await {
                return Json(existing).into_response();
            }
        }
    }
    let currency = input.currency.trim().to_uppercase();
    if currency != "USDC" {
        return error(StatusCode::BAD_REQUEST, "MVP supports USDC checkout only");
    }
    let network = input.network.unwrap_or_else(|| state.network.clone());
    if network != state.network {
        return error(StatusCode::BAD_REQUEST, "unsupported network for this deployment");
    }
    let ttl = input.expires_in.unwrap_or(1800).clamp(60, 86400);
    let now = Utc::now();
    let expires = now + chrono::Duration::seconds(ttl);
    let id = format!("pay_{}", Uuid::new_v4().simple());
    let address = state.receiving_address.clone();
    let payment = Payment {
        id: id.clone(), amount, currency, network,
        reference: input.reference, status: "pending".into(), payment_address: address,
        transaction_hash: None, created_at: now.to_rfc3339(), expires_at: expires.to_rfc3339(),
    };
    let insert = sqlx::query("INSERT INTO payments (id, amount, currency, network, reference, status, payment_address, transaction_hash, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&payment.id).bind(&payment.amount).bind(&payment.currency).bind(&payment.network)
        .bind(&payment.reference).bind(&payment.status).bind(&payment.payment_address)
        .bind(&payment.transaction_hash).bind(&payment.created_at).bind(&payment.expires_at)
        .execute(&state.db).await;
    if let Err(e) = insert { warn!("DB insert failed: {e}"); return error(StatusCode::INTERNAL_SERVER_ERROR, "could not create payment"); }
    if let Some(key) = idempotency_key {
        if let Err(e) = sqlx::query("INSERT INTO idempotency_keys (key, payment_id, created_at) VALUES (?, ?, ?)").bind(key).bind(&payment.id).bind(now.to_rfc3339()).execute(&state.db).await {
            warn!("idempotency insert failed: {e}");
            // A concurrent request may have won the unique-key race; return its payment if present.
            if let Ok(Some(existing_id)) = sqlx::query_scalar::<_, String>("SELECT payment_id FROM idempotency_keys WHERE key = ?").bind(&key).fetch_optional(&state.db).await {
                if let Ok(Some(existing)) = sqlx::query_as::<_, Payment>("SELECT id, amount, currency, network, reference, status, payment_address, transaction_hash, created_at, expires_at FROM payments WHERE id = ?").bind(existing_id).fetch_optional(&state.db).await { return Json(existing).into_response(); }
            }
        }
    }
    (StatusCode::CREATED, Json(payment)).into_response()
}


async fn get_payment(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match sqlx::query_as::<_, Payment>("SELECT id, amount, currency, network, reference, status, payment_address, transaction_hash, created_at, expires_at FROM payments WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await {
        Ok(Some(payment)) => Json(payment).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "payment not found"),
        Err(e) => { warn!("DB query failed: {e}"); error(StatusCode::INTERNAL_SERVER_ERROR, "could not retrieve payment") }
    }
}


async fn dev_confirm(State(state): State<Arc<AppState>>, Path(id): Path<String>, Json(input): Json<DevConfirm>) -> Response {
    if !state.dev_confirmation { return error(StatusCode::NOT_FOUND, "development confirmation is disabled"); }
    if input.transaction_hash.trim().is_empty() { return error(StatusCode::BAD_REQUEST, "transaction_hash is required"); }
    let result = sqlx::query("UPDATE payments SET status = 'confirmed', transaction_hash = ? WHERE id = ? AND status = 'pending'")
        .bind(input.transaction_hash.trim()).bind(&id).execute(&state.db).await;
    match result {
        Ok(res) if res.rows_affected() == 1 => {
            if let Ok(Some(payment)) = sqlx::query_as::<_, Payment>("SELECT id, amount, currency, network, reference, status, payment_address, transaction_hash, created_at, expires_at FROM payments WHERE id = ?").bind(&id).fetch_optional(&state.db).await {
                send_webhook(state, payment.clone(), "payment.confirmed").await;
                Json(payment).into_response()
            } else { error(StatusCode::INTERNAL_SERVER_ERROR, "payment updated but retrieval failed") }
        },
        Ok(_) => error(StatusCode::CONFLICT, "payment not found or not pending"),
        Err(e) => { warn!("DB update failed: {e}"); error(StatusCode::INTERNAL_SERVER_ERROR, "could not confirm payment") }
    }
}


async fn send_webhook(state: Arc<AppState>, payment: Payment, event: &str) {
    let Some(url) = state.webhook_url.clone() else { return; };
    let body = serde_json::json!({"id": format!("evt_{}", Uuid::new_v4().simple()), "event": event, "data": payment});
    let serialized = body.to_string();
    let signature = match HmacSha256::new_from_slice(state.webhook_secret.as_bytes()) {
        Ok(mut mac) => { mac.update(serialized.as_bytes()); hex::encode(mac.finalize().into_bytes()) },
        Err(_) => return,
    };
    // Minimal retry loop. Delivery is currently in-process; a durable outbox is needed for production.
    for attempt in 0..4 {
        let result = state.http.post(&url).header("content-type", "application/json")
            .header("x-fluxpay-signature", format!("sha256={signature}"))
            .body(serialized.clone()).send().await;
        if result.map(|r| r.status().is_success()).unwrap_or(false) { return; }
        tokio::time::sleep(Duration::from_secs(1_u64 << attempt)).await;
    }
    warn!("Webhook delivery failed after retries for {}", payment.id);
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();
    let production = env::var("FLUXPAY_ENV").unwrap_or_else(|_| "development".into()).eq_ignore_ascii_case("production");
    let api_key = env::var("FLUXPAY_API_KEY").unwrap_or_else(|_| "sk_test_change_me".into());
    let webhook_secret = env::var("FLUXPAY_WEBHOOK_SECRET").unwrap_or_else(|_| "whsec_change_me".into());
    if production && (api_key == "sk_test_change_me" || api_key.len() < 32 || webhook_secret == "whsec_change_me" || webhook_secret.len() < 32) {
        return Err("production requires strong FLUXPAY_API_KEY and FLUXPAY_WEBHOOK_SECRET values".into());
    }
    let database_url = env::var("FLUXPAY_DATABASE_URL").unwrap_or_else(|_| "sqlite://fluxpay.db".into());
    let db = SqlitePoolOptions::new().max_connections(5).connect(&database_url).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS payments (id TEXT PRIMARY KEY, amount TEXT NOT NULL, currency TEXT NOT NULL, network TEXT NOT NULL, reference TEXT, status TEXT NOT NULL, payment_address TEXT, transaction_hash TEXT, created_at TEXT NOT NULL, expires_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS idempotency_keys (key TEXT PRIMARY KEY, payment_id TEXT NOT NULL REFERENCES payments(id), created_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS payments_tx_hash_unique ON payments(transaction_hash) WHERE transaction_hash IS NOT NULL").execute(&db).await?;
    let state = Arc::new(AppState {
        db,
        api_key,
        webhook_url: env::var("FLUXPAY_WEBHOOK_URL").ok().filter(|s| !s.trim().is_empty()),
        webhook_secret,
        receiving_address: env::var("FLUXPAY_RECEIVING_ADDRESS").ok().filter(|s| !s.trim().is_empty()),
        network: env::var("FLUXPAY_NETWORK").unwrap_or_else(|_| "base-sepolia".into()),
        dev_confirmation: env::var("FLUXPAY_DEV_CONFIRMATION").map(|v| v == "true").unwrap_or(false),
        http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?,
    });
    let protected = Router::new()
        .route("/v1/payments", post(create_payment))
        .route("/v1/payments/:id", get(get_payment))
        .route("/v1/payments/:id/dev-confirm", post(dev_confirm))
        .layer(middleware::from_fn_with_state(state.clone(), auth));
    let app = Router::new().route("/health", get(health)).route("/ready", get(readiness)).merge(protected).with_state(state);
    let host = env::var("FLUXPAY_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("FLUXPAY_PORT").unwrap_or_else(|_| "8080".into()).parse::<u16>()?;
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("FluxPay listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
