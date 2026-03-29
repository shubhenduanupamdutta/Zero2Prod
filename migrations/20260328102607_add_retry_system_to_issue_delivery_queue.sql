-- Add migration script here
ALTER TABLE issue_delivery_queue
    ADD COLUMN n_retries SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN execute_after TIMESTAMPTZ NOT NULL DEFAULT NOW();
