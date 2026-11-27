# FluxPay

Rust/Axum crypto-payment infrastructure project. **Development only; not production-ready and must not process real funds.**

## Current API scaffold
- Axum/Tokio API and SQLite persistence
- Payment session creation and retrieval
- Shared bearer API key (development)
- Decimal-safe amount parsing
- Idempotency-key support (limited implementation)
- HMAC webhook (in-process retries; not durable)
- Simulated confirmation route, gated by configuration
- Health and database readiness endpoints

## Repository groundwork added
- `docs/PRODUCTION_ROADMAP.md`: architecture and milestone plan
- `docs/THREAT_MODEL.md`: initial threat model and security controls
- `docs/OPERATIONS.md`: local ops and launch gates
- `docker-compose.yml`: local PostgreSQL service (API is not yet connected to it)
- `migrations/0001_initial.sql`: initial PostgreSQL schema draft (not yet wired into API)
- `LICENSE`: MIT

## Run current API
Install Rust stable, copy `.env.example` to `.env`, adjust values, and run:
```sh
cargo run
```

## Endpoints
- `GET /health` - liveness
- `GET /ready` - SQLite readiness
- `POST /v1/payments` - create payment (Bearer key)
- `GET /v1/payments/{id}` - retrieve payment
- `POST /v1/payments/{id}/dev-confirm` - simulated confirmation only when enabled

## Production readiness status
This is an early development scaffold. It is **not ready for real money**. It currently uses SQLite and a single shared bearer key. Payment receiving address is shared and cannot safely attribute simultaneous invoices. Confirmation is simulated, webhooks are in-process, and the API is not wired to the PostgreSQL compose service/migration. Merchant isolation, on-chain verification, reorg handling, durable outbox, rate limits, admin/dashboard, and operational security are outstanding.

Review the roadmap, threat model, and operations docs before deploying.
