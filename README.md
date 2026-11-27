# FluxPay

Rust/Axum crypto-payment API scaffold, intended for **testnet development only**. It is not a production payment processor and does not currently verify on-chain transfers.


## Features
- Axum/Tokio API and SQLite persistence
- Payment sessions for USDC on a configured network label
- Bearer API key
- Decimal-safe amount validation
- Optional persisted idempotency keys for payment creation
- Optional HMAC webhook delivery with in-process retries (not durable)
- Test-only simulated confirmation route
- `/health` liveness and `/ready` database readiness
- Dockerfile and GitHub Actions checks


## Run
Copy `.env.example` to `.env`, change secrets, then run:


```sh
cargo run
```

## API
`GET /health` public liveness endpoint
`GET /ready` public database readiness endpoint
`POST /v1/payments` bearer-authenticated payment creation. Send an optional `Idempotency-Key` header.
`GET /v1/payments/{id}` bearer-authenticated lookup
`POST /v1/payments/{id}/dev-confirm` simulated confirmation only when `FLUXPAY_DEV_CONFIRMATION=true`


### Create payment
```http
POST /v1/payments
Authorization: Bearer sk_test_change_me
Idempotency-Key: order-1029
Content-Type: application/json
```
```json
{
  "amount": "25.00",
  "currency": "USDC",
  "network": "base-sepolia",
  "reference": "order_1029",
  "expires_in": 1800
}
```

## Security and production readiness
Read [`SECURITY.md`](SECURITY.md). Do not accept real funds with this scaffold. On-chain transaction verification, per-merchant authentication, unique payment attribution, durable webhook delivery, PostgreSQL, operational controls, and compliance review are not implemented yet.
