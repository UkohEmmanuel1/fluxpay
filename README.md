# FluxPay

**Rust-first crypto payment infrastructure starter.** FluxPay is an experimental payment API for payment intents, invoices, payment links, customers, webhook endpoint registration, and basic analytics.

> Not production-ready. Do not use to process real funds. On-chain verification, merchant isolation, durable webhook delivery, hosted-checkout security, and production database deployment are not implemented.

## Implemented API surface (prototype)

- `GET /health`, `GET /ready`
- `POST /v1/payments`, `GET /v1/payments`, `GET /v1/payments/{id}`
- `POST /v1/payments/{id}/dev-confirm` (simulation only; explicitly gated)
- `POST /v1/invoices`, `GET /v1/invoices`, `GET /v1/invoices/{id}`
- `GET /checkout/{invoice-id}` (basic HTML invoice display; no chain watcher)
- `POST /v1/payment-links`, `GET /v1/payment-links`
- `POST /v1/customers`, `GET /v1/customers`
- `POST /v1/webhook-endpoints`, `GET /v1/webhook-endpoints`
- `GET /v1/analytics/overview`

Protected routes use a single prototype bearer key from `FLUXPAY_API_KEY`. This is not merchant-scoped authentication. Webhook endpoint records are stored, but event dispatch currently uses the single configured `FLUXPAY_WEBHOOK_URL`; the endpoint registry is not connected to a durable delivery worker.

## Run locally

1. Install Rust stable.
2. Copy `.env.example` to `.env` and use unique secrets.
3. Run `cargo run`.

The starter defaults to SQLite. PostgreSQL compose/migration groundwork is present but not connected to this API.

## Planned production architecture

- Rust/Axum modular monolith with API and durable worker
- PostgreSQL + SQLx migrations
- Merchant isolation, scoped hashed API keys and RBAC
- Transactional idempotency (merchant-scoped and request-hash bound)
- Double-entry ledger with immutable journal records
- Base USDC on-chain monitoring and receipt verification
- Reorg-aware finality policy
- Durable webhook outbox, retry/backoff, dead-letter and replay
- Hosted checkout and QR payment instructions
- TypeScript/Python SDKs, developer portal and sandbox
- Rate limits, observability, backups, security review and compliance readiness

See `docs/IMPLEMENTED_FEATURES.md`, `docs/openapi.yaml`, `docs/PRODUCTION_ROADMAP.md`, `docs/THREAT_MODEL.md`, and `docs/OPERATIONS.md`.

## Safety boundary

The simulated confirmation endpoint is not blockchain verification. The checkout page does not generate a QR code, verify transactions, or automatically settle invoices. Payment links are records only and are not yet interactive checkout sessions. Analytics are basic counts, not accounting or revenue reports. Do not use this API in production or connect it to real-money flows.
