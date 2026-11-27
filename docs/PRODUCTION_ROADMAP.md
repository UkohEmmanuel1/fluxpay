# FluxPay production roadmap

This repository is an engineering foundation, not a production payment processor. No real funds should be accepted until controls below are implemented, reviewed, tested, and deployed.

## Architecture
- Rust API (Axum): merchant API, authentication, payment creation/query
- PostgreSQL: source of truth, migrations, transactional state transitions
- Chain observer: tracks chain head, receipts, logs and reorgs
- Verifier: validates chain ID, token contract, recipient, amount, status, confirmations and duplicates
- Durable outbox: transactionally records webhook events; workers deliver with retries
- Dashboard: merchant configuration, invoices, payments, API keys and webhook delivery logs
- Observability: logs, metrics, traces, alerting and audit records

## Milestones
1. Platform foundation: config, PostgreSQL, migrations, CI, health/readiness
2. Merchant identity: hashed API keys, scopes, revocation, rotation, tenant isolation
3. Payments: lifecycle, merchant-scoped idempotency, payload mismatch, expiry, under/overpayment, late payment
4. Chain: Base Sepolia observer, USDC allowlist, receipt/log verification, cursoring, duplicate handling, reorg support
5. Webhooks: transactional outbox, signed timestamped events, retries, replay prevention, endpoint management
6. Checkout: hosted checkout, QR, polling/SSE and safe expiry UX
7. Dashboard: merchant, payments, keys, webhook logs, reconciliation
8. Operations: rate limits, secrets, TLS, backups/restore, monitoring, runbooks
9. Security: threat model, penetration test, dependency review, key-management review
10. Launch: jurisdiction-specific legal/compliance assessment and controlled pilot

## Payment states
created -> pending -> detected -> confirming -> confirmed. Exception states: expired, underpaid, overpaid, failed, cancelled, late_payment, reorged. Every transition must be validated, recorded, and idempotent. Confirmed state must derive from verified chain data, not client input.


## Current limitations
Existing API uses SQLite and one shared bearer key. Receiving address is shared and cannot reliably attribute concurrent invoices. Confirmation is simulated. Webhook delivery is in-process and not durable. No on-chain RPC verification, reorg handling, merchant isolation, rate limiting, admin system or compliance controls. These limitations block real-money use.
