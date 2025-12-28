-- Contacts table
CREATE TABLE contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    -- Link to XMTP identity (nullable if contact hasn't been matched yet)
    inbox_id TEXT,
    -- Name fields
    display_name TEXT,
    first_name TEXT,
    last_name TEXT,
    prefix TEXT,
    suffix TEXT,
    -- Professional info
    company TEXT,
    job_title TEXT,
    -- Other info
    birthday TEXT,
    note TEXT,
    image_url TEXT,
    -- Status
    is_favorite INTEGER NOT NULL DEFAULT 0,
    -- Timestamps
    created_at_ns BIGINT NOT NULL,
    updated_at_ns BIGINT NOT NULL
);

-- Index for inbox_id lookups
CREATE INDEX idx_contacts_inbox_id ON contacts(inbox_id);

-- Phone numbers companion table
CREATE TABLE contact_phone_numbers (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    phone_number TEXT NOT NULL,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_phone_numbers_contact_id ON contact_phone_numbers(contact_id);

-- Emails companion table
CREATE TABLE contact_emails (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    email TEXT NOT NULL,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_emails_contact_id ON contact_emails(contact_id);

-- URLs companion table
CREATE TABLE contact_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_urls_contact_id ON contact_urls(contact_id);

-- Wallet addresses companion table
CREATE TABLE contact_wallet_addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    wallet_address TEXT NOT NULL,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_wallet_addresses_contact_id ON contact_wallet_addresses(contact_id);

-- Aliases companion table
CREATE TABLE contact_aliases (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    alias TEXT NOT NULL,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_aliases_contact_id ON contact_aliases(contact_id);

-- Street addresses companion table
CREATE TABLE contact_street_addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    street TEXT,
    city TEXT,
    state TEXT,
    postal_code TEXT,
    country TEXT,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_street_addresses_contact_id ON contact_street_addresses(contact_id);
