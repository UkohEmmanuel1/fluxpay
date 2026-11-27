# FluxPay threat model (initial)

## Assets
Payment state, merchant credentials, webhook secrets, observer integrity, database, secrets, merchant metadata.

## Threats
- Forged or replayed payment notifications
- Duplicate transactions credited more than once
- Wrong chain/token/recipient/amount accepted
- Reorganization after apparent confirmation
- RPC outage or stale provider response
- Cross-merchant data access
- API key or webhook-secret theft
- Idempotency abuse and concurrent duplicate requests
- SSRF through merchant webhook URLs
- SQL injection, malformed inputs, resource exhaustion
- Worker crash losing webhook events
- Operator mistakes, leaked secrets, backup compromise

## Required controls
- Merchant API keys stored as salted hashes; scope, rotation, revocation
- Tenant isolation enforced in every query and authorization check
- Merchant-scoped idempotency with request hash; reject key reuse with altered payload
- Unique database constraints for chain/network/transaction/log identity
- Chain ID checks, token allowlists, finality policy, RPC provider strategy
- Reorg-aware event processing
- Webhook SSRF protection: HTTPS, private/link-local/metadata IP blocking, redirect controls, DNS rebinding defense, port restrictions
- Signed webhooks with timestamp/event ID and replay protection guidance
- Durable outbox, exponential backoff with jitter, dead-letter/replay tools
- Rate limits, body limits, timeouts, bounded concurrency
- Secret manager, TLS, least privilege, audit logs, backup/restore tests, alerting
- Independent security review and compliance/legal assessment

Never use real funds, mainnet private keys, or live merchant traffic until launch controls are verified.
