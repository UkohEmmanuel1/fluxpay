-- Initial database groundwork; API is not yet wired to this schema.
CREATE TABLE IF NOT EXISTS payments (
  id TEXT PRIMARY KEY,
  amount NUMERIC(38, 18) NOT NULL CHECK (amount > 0),
  currency TEXT NOT NULL,
  network TEXT NOT NULL,
  reference TEXT,
  status TEXT NOT NULL CHECK (status IN ('pending','detected','confirming','confirmed','expired','underpaid','overpaid','failed','cancelled','late_payment','reorged')),
  payment_address TEXT,
  transaction_hash TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS payments_status_created_idx ON payments(status, created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS payments_transaction_hash_unique ON payments(transaction_hash) WHERE transaction_hash IS NOT NULL;
