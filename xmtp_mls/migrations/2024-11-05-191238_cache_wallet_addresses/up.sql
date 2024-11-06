CREATE TABLE wallet_addresses(
    inbox_id TEXT NOT NULL,
    wallet_address TEXT NOT NULL,
    PRIMARY KEY (inbox_id, wallet_address)
)
