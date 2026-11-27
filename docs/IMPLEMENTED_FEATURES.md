# FluxPay feature inventory

This file distinguishes implemented prototype behavior from roadmap items. Implemented items are not production-safe without review and test coverage.

## Implemented prototype API

- Payment creation and lookup
- Payment list (latest 100)
- Decimal string validation (positive, at most 6 fractional digits)
- USDC symbol guard and configured-network guard
- Idempotency key persistence (prototype-level; no merchant scope or payload-hash mismatch protection)
- Development-only simulated confirmation
- Single configured webhook URL with in-process retries and HMAC SHA-256 signature
- Invoice records, list/get, basic public HTML checkout display
- Payment link records (fixed or open amount), list, public informational landing page
- Customer records with JSON metadata, list
- Webhook endpoint registration records, list (registration is not connected to delivery)
- Basic analytics counts and confirmation ratio
- Liveness and database readiness endpoints
- SQLite runtime schema bootstrap
- OpenAPI contract for the current API

## Public routes

- `GET /health`
- `GET /ready`
- `GET /checkout/{invoice_id}`
- `GET /pay/{slug}`

## Authenticated routes

Use prototype bearer authentication with `FLUXPAY_API_KEY`.

- `POST /v1/payments`
- `GET /v1/payments`
- `GET /v1/payments/{id}`
- `POST /v1/payments/{id}/dev-confirm`
- `POST /v1/invoices`
- `GET /v1/invoices`
- `GET /v1/invoices/{id}`
- `POST /v1/payment-links`
- `GET /v1/payment-links`
- `POST /v1/customers`
- `GET /v1/customers`
- `POST /v1/webhook-endpoints`
- `GET /v1/webhook-endpoints`
- `GET /v1/analytics/overview`

## Not yet implemented

- Actual checkout payment submission or wallet connect
- QR code generation
- Chain RPC monitoring and event indexing
- Verified payment detection, token/recipient validation, confirmations/finality or reorg handling
- Automatic invoice/link payment matching
- Merchant accounts, isolation, scoped keys, roles, dashboard UI or admin console
- Durable webhook outbox, endpoint-specific delivery, retry jobs, replay UI or dead-letter queue
- Double-entry ledger, settlement, payouts, refunds, fees, risk scoring or reconciliation
- Production Postgres runtime, automated migrations, Redis/queue
- SDKs, developer portal, sandbox isolation
- Rate limiting, robust SSRF controls, full observability, production deployment hardening

**No real payment is verified by this prototype.** The development-confirmation endpoint is a simulation only. Do not use this software to receive, custody, settle, or account for real funds.
