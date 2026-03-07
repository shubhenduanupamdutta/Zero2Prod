-- Add migration script here
ALTER TABLE subscription_tokens ADD COLUMN consumed_at timestamptz;
