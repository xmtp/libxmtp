ALTER TABLE outbound_payloads
ADD COLUMN locked_until_ns BIGINT NOT NULL DEFAULT 0;