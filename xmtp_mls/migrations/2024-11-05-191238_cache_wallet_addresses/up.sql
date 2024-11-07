CREATE TABLE wallet_addresses(
    inbox_id TEXT NOT NULL,
    wallet_address TEXT PRIMARY KEY NOT NULL
);

CREATE INDEX idx_wallet_inbox_id ON wallet_addresses(inbox_id);