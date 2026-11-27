-- PostgreSQL groundwork for FluxPay prototype modules.
-- This migration is documentation/foundation only: current API runtime is SQLite
-- and does not execute these migrations automatically.
CREATE TABLE IF NOT EXISTS invoices (
  id TEXT PRIMARY KEY,
  amount NUMERIC(38,18) NOT NULL CHECK (amount > 0),
  currency TEXT NOT NULL,
  description TEXT,
  customer_email TEXT,
  reference TEXT,
  status TEXT NOT NULL,
  checkout_path TEXT NOT NULL,
  payment_address TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS invoices_status_created_idx ON invoices(status, created_at DESC);

CREATE TABLE IF NOT EXISTS payment_links (
  id TEXT PRIMARY KEY,
  slug TEXT UNIQUE NOT NULL,
  amount NUMERIC(38,18),
  currency TEXT NOT NULL,
  title TEXT,
  description TEXT,
  reference TEXT,
  status TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS payment_links_status_created_idx ON payment_links(status, created_at DESC);

CREATE TABLE IF NOT EXISTS customers (
  id TEXT PRIMARY KEY,
  email TEXT,
  name TEXT,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS webhook_endpoints (
  id TEXT PRIMARY PRIMARY KEY,
  url TEXT NOT NULL,
  secret TEXT NOT NULL,
  events JSONB NOT NULL DEFAULT '[]'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
