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
    id: String, amount: String, currency: String, network: String,
    reference: Option<String>, status: String, payment_address: Option<String>,
    transaction_hash: Option<String>, created_at: String, expires_at: String,
}

#[derive(Debug, Deserialize)]
struct CreatePayment {
    amount: String, currency: String, network: Option<String>,
    reference: Option<String>, expires_in: Option<i64>,
}
#[derive(Debug, Deserialize)]
struct DevConfirm { transaction_hash: String }
#[derive(Debug, Deserialize)]
struct CreateInvoice {
    amount: String, currency: Option<String>, description: Option<String>,
    customer_email: Option<String>, reference: Option<String>, expires_in: Option<i64>,
}
#[derive(Debug, Deserialize)]
struct CreateLink {
    amount: Option<String>, currency: Option<String>, title: Option<String>,
    description: Option<String>, reference: Option<String>, expires_in: Option<i64>,
}
#[derive(Debug, Deserialize)]
struct CreateCustomer { email: Option<String>, name: Option<String>, metadata: Option<serde_json::Value> }
#[derive(Debug, Deserialize)]
struct CreateWebhook { url: String, events: Option<Vec<String>> }

#[derive(Debug, Serialize)]
struct ApiError { error: String }
fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ApiError { error: message.into() })).into_response()
}
async fn auth(State(state): State<Arc<AppState>>, request: Request<axum::body::Body>, next: Next) -> Response {
    let expected = format!("Bearer {}", state.api_key);
    let valid = request.headers().get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok()).map(|v| v == expected).unwrap_or(false);
    if valid { next.run(request).await } else { error(StatusCode::UNAUTHORIZED, "Missing or invalid API key") }
}
async fn health() -> impl IntoResponse { Json(serde_json::json!({"status":"ok","service":"fluxpay","version":"0.3.0"})) }
async fn readiness(State(s): State<Arc<AppState>>) -> Response {
    match sqlx::query("SELECT 1").execute(&s.db).await {
        Ok(_) => Json(serde_json::json!({"status":"ready","database":"ok"})).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"not_ready","database":"unavailable"}))).into_response(),
    }
}
fn valid_amount(raw: &str) -> Option<String> {
    match raw.trim().parse::<rust_decimal::Decimal>() {
        Ok(v) if v.is_sign_positive() && !v.is_zero() && v.scale() <= 6 && v.to_string().len() <= 32 => Some(v.normalize().to_string()),
        _ => None
    }
}
async fn create_payment(State(s): State<Arc<AppState>>, headers: HeaderMap, Json(input): Json<CreatePayment>) -> Response {
    let Some(amount) = valid_amount(&input.amount) else { return error(StatusCode::BAD_REQUEST, "amount must be a positive decimal with at most 6 fractional digits"); };
    let currency = input.currency.trim().to_uppercase();
    if currency != "USDC" { return error(StatusCode::BAD_REQUEST, "MVP supports USDC only"); }
    let network = input.network.unwrap_or_else(|| s.network.clone());
    if network != s.network { return error(StatusCode::BAD_REQUEST, "unsupported network for this deployment"); }
    let key = headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    if let Some(k) = &key {
        if k.len() > 128 { return error(StatusCode::BAD_REQUEST, "idempotency-key too long"); }
        if let Ok(Some(id)) = sqlx::query_scalar::<_, String>("SELECT payment_id FROM idempotency_keys WHERE key=?").bind(k).fetch_optional(&s.db).await {
            if let Ok(Some(p)) = sqlx::query_as::<_, Payment>("SELECT * FROM payments WHERE id=?").bind(id).fetch_optional(&s.db).await { return Json(p).into_response(); }
        }
    }
    let ttl = input.expires_in.unwrap_or(1800).clamp(60, 86400);
    let now = Utc::now(); let expires = now + chrono::Duration::seconds(ttl);
    let p = Payment { id: format!("pay_{}", Uuid::new_v4().simple()), amount, currency, network,
        reference: input.reference, status: "pending".into(), payment_address: s.receiving_address.clone(),
        transaction_hash: None, created_at: now.to_rfc3339(), expires_at: expires.to_rfc3339() };
    let mut tx = match s.db.begin().await { Ok(t)=>t, Err(e)=>{warn!("{e}");return error(StatusCode::INTERNAL_SERVER_ERROR,"database unavailable");}};
    if let Err(e) = sqlx::query("INSERT INTO payments (id,amount,currency,network,reference,status,payment_address,transaction_hash,created_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&p.id).bind(&p.amount).bind(&p.currency).bind(&p.network).bind(&p.reference).bind(&p.status).bind(&p.payment_address).bind(&p.transaction_hash).bind(&p.created_at).bind(&p.expires_at).execute(&mut *tx).await {
        warn!("{e}"); return error(StatusCode::INTERNAL_SERVER_ERROR,"payment insert failed");
    }
    if let Some(k)=key {
        if let Err(e)=sqlx::query("INSERT INTO idempotency_keys (key,payment_id,created_at) VALUES (?,?,?)").bind(k).bind(&p.id).bind(now.to_rfc3339()).execute(&mut *tx).await {
            warn!("{e}"); return error(StatusCode::CONFLICT,"idempotency key already used");
        }
    }
    if let Err(e)=tx.commit().await {warn!("{e}");return error(StatusCode::INTERNAL_SERVER_ERROR,"transaction commit failed");}
    (StatusCode::CREATED,Json(p)).into_response()
}
async fn get_payment(State(s):State<Arc<AppState>>,Path(id):Path<String>)->Response {
    match sqlx::query_as::<_,Payment>("SELECT * FROM payments WHERE id=?").bind(id).fetch_optional(&s.db).await {
        Ok(Some(p))=>Json(p).into_response(), Ok(None)=>error(StatusCode::NOT_FOUND,"payment not found"),
        Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"query failed")}
    }
}
async fn list_payments(State(s):State<Arc<AppState>>)->Response {
    match sqlx::query_as::<_,Payment>("SELECT * FROM payments ORDER BY created_at DESC LIMIT 100").fetch_all(&s.db).await {
        Ok(items)=>Json(items).into_response(), Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"query failed")}
    }
}
async fn dev_confirm(State(s):State<Arc<AppState>>,Path(id):Path<String>,Json(input):Json<DevConfirm>)->Response {
    if !s.dev_confirmation { return error(StatusCode::NOT_FOUND,"development confirmation disabled"); }
    if input.transaction_hash.trim().is_empty() { return error(StatusCode::BAD_REQUEST,"transaction_hash required"); }
    let r=sqlx::query("UPDATE payments SET status='confirmed',transaction_hash=? WHERE id=? AND status='pending'").bind(input.transaction_hash.trim()).bind(&id).execute(&s.db).await;
    match r {
        Ok(r) if r.rows_affected()==1=>match sqlx::query_as::<_,Payment>("SELECT * FROM payments WHERE id=?").bind(id).fetch_optional(&s.db).await {
            Ok(Some(p))=>{send_webhook(s,p.clone(),"payment.confirmed").await;Json(p).into_response()},_=>error(StatusCode::INTERNAL_SERVER_ERROR,"payment retrieval failed")},
        Ok(_)=>error(StatusCode::CONFLICT,"not found or not pending"),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"confirmation failed")}
    }
}
async fn send_webhook(s:Arc<AppState>,p:Payment,event:&str) {
    let Some(url)=s.webhook_url.clone() else{return;};
    let body=serde_json::json!({"id":format!("evt_{}",Uuid::new_v4().simple()),"event":event,"data":p});
    let text=body.to_string();
    let sig=match HmacSha256::new_from_slice(s.webhook_secret.as_bytes()){Ok(mut m)=>{m.update(text.as_bytes());hex::encode(m.finalize().into_bytes())},Err(_)=>return};
    for i in 0..4 {let result=s.http.post(&url).header("content-type","application/json").header("x-fluxpay-signature",format!("sha256={sig}")).body(text.clone()).send().await;
        if result.map(|r|r.status().is_success()).unwrap_or(false){return;}tokio::time::sleep(Duration::from_secs(1<<i)).await;}
    warn!("webhook delivery exhausted for {}",p.id);
}
async fn create_invoice(State(s):State<Arc<AppState>>,Json(i):Json<CreateInvoice>)->Response {
    let Some(amount)=valid_amount(&i.amount) else{return error(StatusCode::BAD_REQUEST,"invalid amount");};
    let currency=i.currency.unwrap_or_else(||"USD".into()).to_uppercase();
    let now=Utc::now();let expiry=now+chrono::Duration::seconds(i.expires_in.unwrap_or(3600).clamp(60,86400));
    let id=format!("inv_{}",Uuid::new_v4().simple());
    let checkout=format!("/checkout/{id}");
    let r=sqlx::query("INSERT INTO invoices(id,amount,currency,description,customer_email,reference,status,checkout_path,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
      .bind(&id).bind(amount).bind(currency).bind(i.description).bind(i.customer_email).bind(i.reference).bind("open").bind(&checkout).bind(now.to_rfc3339()).bind(expiry.to_rfc3339()).execute(&s.db).await;
    match r{Ok(_)=>(StatusCode::CREATED,Json(serde_json::json!({"id":id,"status":"open","checkout_path":checkout,"created_at":now.to_rfc3339(),"expires_at":expiry.to_rfc3339()}))).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"invoice creation failed")}}
}
async fn list_invoices(State(s):State<Arc<AppState>>)->Response {
    match sqlx::query_as::<_,InvoiceRow>("SELECT id,amount,currency,description,customer_email,reference,status,checkout_path,created_at,expires_at FROM invoices ORDER BY created_at DESC LIMIT 100").fetch_all(&s.db).await {
        Ok(rows)=>Json(rows).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"invoice query failed")}
    }
}
async fn get_invoice(State(s):State<Arc<AppState>>,Path(id):Path<String>)->Response {
    match sqlx::query_as::<_,InvoiceRow>("SELECT id,amount,currency,description,customer_email,reference,status,checkout_path,created_at,expires_at FROM invoices WHERE id=?").bind(id).fetch_optional(&s.db).await {
        Ok(Some(row))=>Json(row).into_response(),Ok(None)=>error(StatusCode::NOT_FOUND,"invoice not found"),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"invoice query failed")}
    }
}
async fn create_payment_link(State(s):State<Arc<AppState>>,Json(i):Json<CreateLink>)->Response {
    let amount=match i.amount{Some(a)=>match valid_amount(&a){Some(v)=>Some(v),None=>return error(StatusCode::BAD_REQUEST,"invalid amount")},None=>None};
    let now=Utc::now();let expiry=now+chrono::Duration::seconds(i.expires_in.unwrap_or(86400).clamp(60,2592000));
    let id=format!("plink_{}",Uuid::new_v4().simple());let slug=Uuid::new_v4().simple().to_string();
    let r=sqlx::query("INSERT INTO payment_links(id,slug,amount,currency,title,description,reference,status,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
      .bind(&id).bind(&slug).bind(amount).bind(i.currency.unwrap_or_else(||"USD".into()).to_uppercase()).bind(i.title).bind(i.description).bind(i.reference).bind("active").bind(now.to_rfc3339()).bind(expiry.to_rfc3339()).execute(&s.db).await;
    match r{Ok(_)=>(StatusCode::CREATED,Json(serde_json::json!({"id":id,"slug":slug,"url":format!("/pay/{slug}"),"status":"active"}))).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"payment link creation failed")}}
}
async fn list_links(State(s):State<Arc<AppState>>)->Response {
    match sqlx::query_as::<_,PaymentLinkRow>("SELECT id,slug,amount,currency,title,description,reference,status,created_at,expires_at FROM payment_links ORDER BY created_at DESC LIMIT 100").fetch_all(&s.db).await{Ok(r)=>Json(r).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"query failed")}}
}
async fn create_customer(State(s):State<Arc<AppState>>,Json(i):Json<CreateCustomer>)->Response {
    let id=format!("cus_{}",Uuid::new_v4().simple());let now=Utc::now().to_rfc3339();
    match sqlx::query("INSERT INTO customers(id,email,name,metadata,created_at) VALUES(?,?,?,?,?)").bind(&id).bind(i.email).bind(i.name).bind(i.metadata.unwrap_or(serde_json::json!({})).to_string()).bind(now).execute(&s.db).await {
        Ok(_)=>(StatusCode::CREATED,Json(serde_json::json!({"id":id,"created_at":now}))).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"customer create failed")}
    }
}
async fn list_customers(State(s):State<Arc<AppState>>)->Response {
    match sqlx::query_as::<_,CustomerRow>("SELECT id,email,name,metadata,created_at FROM customers ORDER BY created_at DESC LIMIT 100").fetch_all(&s.db).await{Ok(r)=>Json(r).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"customer query failed")}}
}
async fn create_webhook_endpoint(State(s):State<Arc<AppState>>,Json(i):Json<CreateWebhook>)->Response {
    if !i.url.starts_with("https://"){return error(StatusCode::BAD_REQUEST,"webhook URL must use HTTPS");}
    let id=format!("we_{}",Uuid::new_v4().simple());let secret=format!("whsec_{}",Uuid::new_v4().simple());let now=Utc::now().to_rfc3339();
    match sqlx::query("INSERT INTO webhook_endpoints(id,url,secret,events,created_at) VALUES(?,?,?,?,?)").bind(&id).bind(i.url).bind(&secret).bind(serde_json::to_string(&i.events.unwrap_or_default()).unwrap_or_default()).bind(now).execute(&s.db).await {
        Ok(_)=>(StatusCode::CREATED,Json(serde_json::json!({"id":id,"secret":secret,"created_at":now}))).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"webhook endpoint creation failed")}
    }
}
async fn list_webhook_endpoints(State(s):State<Arc<AppState>>)->Response {
    match sqlx::query_as::<_,WebhookEndpointRow>("SELECT id,url,events,created_at FROM webhook_endpoints ORDER BY created_at DESC").fetch_all(&s.db).await{Ok(r)=>Json(r).into_response(),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"webhook endpoint query failed")}}
}
async fn public_payment_link(State(s):State<Arc<AppState>>,Path(slug):Path<String>)->Response {
    match sqlx::query("SELECT title,description,amount,currency,status,expires_at FROM payment_links WHERE slug=?").bind(slug).fetch_optional(&s.db).await {
        Ok(Some(row))=>{
            use sqlx::Row;
            let title=row.try_get::<Option<String>,_>("title").unwrap_or(None).unwrap_or_else(||"Payment".into());
            let description=row.try_get::<Option<String>,_>("description").unwrap_or(None).unwrap_or_default();
            let amount=row.try_get::<Option<String>,_>("amount").unwrap_or(None);
            let currency=row.try_get::<String,_>("currency").unwrap_or_else(|_|"USD".into());
            let status=row.try_get::<String,_>("status").unwrap_or_default();
            let expires=row.try_get::<String,_>("expires_at").unwrap_or_default();
            let amount_display=amount.map(|a|format!("{} {}",a,currency)).unwrap_or_else(||format!("Enter amount in {}",currency));
            let html=format!("<!doctype html><html><head><meta name='viewport' content='width=device-width'><title>FluxPay payment link</title></head><body style='font-family:system-ui;max-width:560px;margin:4rem auto;padding:1rem;background:#0b1020;color:#fff'><h1>{}</h1><p>{}</p><h2>{}</h2><p>Status: {}</p><p>Expires: {}</p><p>This payment link is a prototype record. Payment submission and verification are not available yet.</p></body></html>",escape_html(&title),escape_html(&description),escape_html(&amount_display),escape_html(&status),escape_html(&expires));
            ([(axum::http::header::CONTENT_TYPE,"text/html; charset=utf-8")],html).into_response()
        },
        Ok(None)=>error(StatusCode::NOT_FOUND,"payment link not found"),
        Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"payment link lookup failed")}
    }
}
async fn analytics(State(s):State<Arc<AppState>>)->Response {
    let total=sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM payments").fetch_one(&s.db).await.unwrap_or(0);
    let confirmed=sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM payments WHERE status='confirmed'").fetch_one(&s.db).await.unwrap_or(0);
    let pending=sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM payments WHERE status='pending'").fetch_one(&s.db).await.unwrap_or(0);
    Json(serde_json::json!({"total_payments":total,"confirmed_payments":confirmed,"pending_payments":pending,"confirmation_rate":if total>0 {confirmed as f64/total as f64} else {0.0}})).into_response()
}
fn escape_html(value: &str) -> String { value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;") }
async fn checkout_page(State(s):State<Arc<AppState>>,Path(id):Path<String>)->Response {
    match sqlx::query("SELECT amount,currency,status,payment_address,expires_at FROM invoices WHERE id=?").bind(id).fetch_optional(&s.db).await {
        Ok(Some(row))=>{
            use sqlx::Row;
            let amount=row.try_get::<String,_>("amount").unwrap_or_default();let currency=row.try_get::<String,_>("currency").unwrap_or_default();
            let status=row.try_get::<String,_>("status").unwrap_or_default();let address=row.try_get::<Option<String>,_>("payment_address").unwrap_or(None).or_else(||s.receiving_address.clone()).unwrap_or_default();
            let expires=row.try_get::<String,_>("expires_at").unwrap_or_default();
            let html=format!("<!doctype html><html><head><meta name='viewport' content='width=device-width'><title>FluxPay Checkout</title></head><body style='font-family:system-ui;max-width:560px;margin:4rem auto;padding:1rem;background:#0b1020;color:#fff'><h1>FluxPay Checkout</h1><p>Invoice status: {}</p><h2>{}</h2><p>Network: {}</p><p>Send payment to:</p><code style='word-break:break-all'>{}</code><p>Expires: {}</p><p>Payment verification is not automatic in this starter.</p></body></html>",escape_html(&status),escape_html(&format!("{amount} {currency}")),escape_html(&s.network),escape_html(&address),escape_html(&expires));
            ([(axum::http::header::CONTENT_TYPE,"text/html; charset=utf-8")],html).into_response()
        },Ok(None)=>error(StatusCode::NOT_FOUND,"invoice not found"),Err(e)=>{warn!("{e}");error(StatusCode::INTERNAL_SERVER_ERROR,"checkout unavailable")}
    }
}
#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();tracing_subscriber::fmt().with_env_filter("info").init();
    let prod=env::var("FLUXPAY_ENV").unwrap_or_else(|_|"development".into()).eq_ignore_ascii_case("production");
    let api_key=env::var("FLUXPAY_API_KEY").unwrap_or_else(|_|"sk_test_change_me".into());
    let webhook_secret=env::var("FLUXPAY_WEBHOOK_SECRET").unwrap_or_else(|_|"whsec_change_me".into());
    if prod&&(api_key=="sk_test_change_me"||api_key.len()<32||webhook_secret=="whsec_change_me"||webhook_secret.len()<32){return Err("production requires strong API and webhook secrets".into());}
    let db_url=env::var("FLUXPAY_DATABASE_URL").unwrap_or_else(|_|"sqlite://fluxpay.db".into());
    let db=SqlitePoolOptions::new().max_connections(5).connect(&db_url).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS payments(id TEXT PRIMARY KEY,amount TEXT NOT NULL,currency TEXT NOT NULL,network TEXT NOT NULL,reference TEXT,status TEXT NOT NULL,payment_address TEXT,transaction_hash TEXT,created_at TEXT NOT NULL,expires_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS idempotency_keys(key TEXT PRIMARY KEY,payment_id TEXT NOT NULL REFERENCES payments(id),created_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS payments_tx_hash_unique ON payments(transaction_hash) WHERE transaction_hash IS NOT NULL").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS invoices(id TEXT PRIMARY KEY,amount TEXT NOT NULL,currency TEXT NOT NULL,description TEXT,customer_email TEXT,reference TEXT,status TEXT NOT NULL,checkout_path TEXT NOT NULL,payment_address TEXT,created_at TEXT NOT NULL,expires_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS payment_links(id TEXT PRIMARY KEY,slug TEXT UNIQUE NOT NULL,amount TEXT,currency TEXT NOT NULL,title TEXT,description TEXT,reference TEXT,status TEXT NOT NULL,created_at TEXT NOT NULL,expires_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS customers(id TEXT PRIMARY KEY,email TEXT,name TEXT,metadata TEXT NOT NULL,created_at TEXT NOT NULL)").execute(&db).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS webhook_endpoints(id TEXT PRIMARY KEY,url TEXT NOT NULL,secret TEXT NOT NULL,events TEXT NOT NULL,created_at TEXT NOT NULL)").execute(&db).await?;
    let s=Arc::new(AppState{db,api_key,webhook_url:env::var("FLUXPAY_WEBHOOK_URL").ok().filter(|x|!x.trim().is_empty()),webhook_secret,receiving_address:env::var("FLUXPAY_RECEIVING_ADDRESS").ok().filter(|x|!x.trim().is_empty()),network:env::var("FLUXPAY_NETWORK").unwrap_or_else(|_|"base-sepolia".into()),dev_confirmation:env::var("FLUXPAY_DEV_CONFIRMATION").map(|x|x=="true").unwrap_or(false),http:reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?});
    let protected=Router::new()
      .route("/v1/payments",post(create_payment).get(list_payments))
      .route("/v1/payments/:id",get(get_payment))
      .route("/v1/payments/:id/dev-confirm",post(dev_confirm))
      .route("/v1/invoices",post(create_invoice).get(list_invoices))
      .route("/v1/invoices/:id",get(get_invoice))
      .route("/v1/payment-links",post(create_payment_link).get(list_links))
      .route("/v1/customers",post(create_customer).get(list_customers))
      .route("/v1/webhook-endpoints",post(create_webhook_endpoint).get(list_webhook_endpoints))
      .route("/v1/analytics/overview",get(analytics))
      .layer(middleware::from_fn_with_state(s.clone(),auth));
    let public=Router::new().route("/checkout/:id",get(checkout_page)).route("/pay/:slug",get(public_payment_link));
    let app=Router::new().route("/health",get(health)).route("/ready",get(readiness)).merge(public).merge(protected).with_state(s);
    let host=env::var("FLUXPAY_HOST").unwrap_or_else(|_|"127.0.0.1".into());let port=env::var("FLUXPAY_PORT").unwrap_or_else(|_|"8080".into()).parse::<u16>()?;
    let addr:SocketAddr=format!("{host}:{port}").parse()?;let listener=tokio::net::TcpListener::bind(addr).await?;
    info!("FluxPay listening at {}",addr);axum::serve(listener,app).await?;Ok(())
}
