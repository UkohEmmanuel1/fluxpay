# FluxPay security status

FluxPay is a development foundation. It is **not ready to handle real funds**.


## Before any real-money use
- Replace the single shared API key with per-merchant keys stored hashed, scoped, revocable, and rotated.
- Implement real chain receipt/log verification against a trusted RPC provider; validate chain ID, token contract, recipient, amount, receipt status, confirmations/finality, and reorgs.
- Generate unique payment identifiers/address strategy. A shared receiving address is not safe for matching concurrent invoices without robust on-chain matching.
- Move persistence to PostgreSQL, apply versioned migrations, and enforce transactional payment + idempotency writes.
- Add durable webhook outbox, delivery attempts, retry scheduling, signing timestamp, replay protection, and per-merchant secrets.
- Implement rate limits, request size limits, schema validation, CORS policy, TLS termination, audit logs, secret management, alerting, metrics, backups and restore drills.
- Remove/disable simulated confirmation in production and verify route is not exposed.
- Perform independent security review and assess applicable financial, AML/KYC, sanctions, privacy and consumer-protection requirements.

## Current safeguards
- Decimal string amount parsing (no floating point) and bounded decimal scale.
- Bearer key authentication for API routes.
- Persisted idempotency-key support for payment creation (limited: key namespace is global and payload mismatch is not detected).
- Unique transaction hash index.
- Production startup refuses default/short API and webhook secrets.
- Readiness endpoint checks database connectivity.
- Outbound HTTP request timeout.

These safeguards are not a substitute for the outstanding controls above.
