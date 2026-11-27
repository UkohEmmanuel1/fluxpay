# Local operations

## Start PostgreSQL
Copy .env.example to .env and change secret values. Start the local database:

```sh
docker compose up -d postgres
```

Important: the current Rust API still uses SQLite. The compose file and migration are groundwork and are not wired to the API yet. Do not deploy this stack for real payment processing.

## Launch gates
- Managed PostgreSQL, backups and tested restoration
- Secrets via platform secret manager; never commit .env
- TLS at trusted edge
- Restricted database network access
- Monitoring, logs, alerts and incident response
- CI tests/build and independent security review must pass
