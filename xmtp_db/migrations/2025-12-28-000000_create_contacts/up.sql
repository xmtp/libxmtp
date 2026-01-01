-- Contacts table
CREATE TABLE contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    -- Link to XMTP identity
    inbox_id TEXT NOT NULL,
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
    -- Timestamps (managed by application layer with nanosecond precision)
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

-- Addresses companion table
CREATE TABLE contact_addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    contact_id INTEGER NOT NULL,
    address1 TEXT,
    address2 TEXT,
    address3 TEXT,
    city TEXT,
    region TEXT,
    postal_code TEXT,
    country TEXT,
    label TEXT,
    FOREIGN KEY (contact_id) REFERENCES contacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contact_addresses_contact_id ON contact_addresses(contact_id);

-- View for full contact with all related data
CREATE VIEW contact_list AS
SELECT
    c.inbox_id,
    c.display_name,
    c.first_name,
    c.last_name,
    c.prefix,
    c.suffix,
    c.company,
    c.job_title,
    c.birthday,
    c.note,
    c.image_url,
    c.is_favorite,
    c.created_at_ns,
    c.updated_at_ns,
    (SELECT COALESCE(json_group_array(json_object('id', id, 'phone_number', phone_number, 'label', label)), '[]')
     FROM contact_phone_numbers WHERE contact_id = c.id) as phone_numbers,
    (SELECT COALESCE(json_group_array(json_object('id', id, 'email', email, 'label', label)), '[]')
     FROM contact_emails WHERE contact_id = c.id) as emails,
    (SELECT COALESCE(json_group_array(json_object('id', id, 'url', url, 'label', label)), '[]')
     FROM contact_urls WHERE contact_id = c.id) as urls,
    (SELECT COALESCE(json_group_array(json_object('id', id, 'wallet_address', wallet_address, 'label', label)), '[]')
     FROM contact_wallet_addresses WHERE contact_id = c.id) as wallet_addresses,
    (SELECT COALESCE(json_group_array(json_object('id', id, 'address1', address1, 'address2', address2, 'address3', address3, 'city', city, 'region', region, 'postal_code', postal_code, 'country', country, 'label', label)), '[]')
     FROM contact_addresses WHERE contact_id = c.id) as addresses
FROM contacts c;

-- FTS5 full-text search index for contacts
-- Using trigram tokenizer for substring matching anywhere in text
-- detail='full' required for trigram phrase queries
CREATE VIRTUAL TABLE contacts_fts USING fts5(
    inbox_id UNINDEXED,
    searchable_text,
    tokenize='trigram'
);

-- Helper function to build searchable text for a contact
-- We use a trigger-based approach to keep FTS in sync

-- Trigger: After INSERT on contacts
CREATE TRIGGER contacts_fts_insert AFTER INSERT ON contacts BEGIN
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    VALUES (
        NEW.inbox_id,
        COALESCE(NEW.display_name, '') || ' ' ||
        COALESCE(NEW.first_name, '') || ' ' ||
        COALESCE(NEW.last_name, '') || ' ' ||
        COALESCE(NEW.prefix, '') || ' ' ||
        COALESCE(NEW.suffix, '') || ' ' ||
        COALESCE(NEW.company, '') || ' ' ||
        COALESCE(NEW.job_title, '') || ' ' ||
        COALESCE(NEW.note, '')
    );
END;

-- Trigger: After UPDATE on contacts
CREATE TRIGGER contacts_fts_update AFTER UPDATE ON contacts BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = OLD.inbox_id;
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        NEW.inbox_id,
        COALESCE(NEW.display_name, '') || ' ' ||
        COALESCE(NEW.first_name, '') || ' ' ||
        COALESCE(NEW.last_name, '') || ' ' ||
        COALESCE(NEW.prefix, '') || ' ' ||
        COALESCE(NEW.suffix, '') || ' ' ||
        COALESCE(NEW.company, '') || ' ' ||
        COALESCE(NEW.job_title, '') || ' ' ||
        COALESCE(NEW.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = NEW.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = NEW.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = NEW.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = NEW.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = NEW.id), '');
END;

-- Trigger: After DELETE on contacts
CREATE TRIGGER contacts_fts_delete AFTER DELETE ON contacts BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = OLD.inbox_id;
END;

-- Triggers for companion tables: rebuild FTS entry when companion data changes

-- Phone numbers
CREATE TRIGGER contact_phone_numbers_fts_insert AFTER INSERT ON contact_phone_numbers BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_phone_numbers_fts_update AFTER UPDATE ON contact_phone_numbers BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_phone_numbers_fts_delete AFTER DELETE ON contact_phone_numbers BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = OLD.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = OLD.contact_id;
END;

-- Emails
CREATE TRIGGER contact_emails_fts_insert AFTER INSERT ON contact_emails BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_emails_fts_update AFTER UPDATE ON contact_emails BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_emails_fts_delete AFTER DELETE ON contact_emails BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = OLD.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = OLD.contact_id;
END;

-- URLs
CREATE TRIGGER contact_urls_fts_insert AFTER INSERT ON contact_urls BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_urls_fts_update AFTER UPDATE ON contact_urls BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_urls_fts_delete AFTER DELETE ON contact_urls BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = OLD.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = OLD.contact_id;
END;

-- Wallet addresses
CREATE TRIGGER contact_wallet_addresses_fts_insert AFTER INSERT ON contact_wallet_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_wallet_addresses_fts_update AFTER UPDATE ON contact_wallet_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_wallet_addresses_fts_delete AFTER DELETE ON contact_wallet_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = OLD.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = OLD.contact_id;
END;

-- Addresses
CREATE TRIGGER contact_addresses_fts_insert AFTER INSERT ON contact_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_addresses_fts_update AFTER UPDATE ON contact_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = NEW.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = NEW.contact_id;
END;

CREATE TRIGGER contact_addresses_fts_delete AFTER DELETE ON contact_addresses BEGIN
    DELETE FROM contacts_fts WHERE inbox_id = (SELECT inbox_id FROM contacts WHERE id = OLD.contact_id);
    INSERT INTO contacts_fts(inbox_id, searchable_text)
    SELECT
        c.inbox_id,
        COALESCE(c.display_name, '') || ' ' ||
        COALESCE(c.first_name, '') || ' ' ||
        COALESCE(c.last_name, '') || ' ' ||
        COALESCE(c.prefix, '') || ' ' ||
        COALESCE(c.suffix, '') || ' ' ||
        COALESCE(c.company, '') || ' ' ||
        COALESCE(c.job_title, '') || ' ' ||
        COALESCE(c.note, '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(phone_number, ' ') FROM contact_phone_numbers WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(email, ' ') FROM contact_emails WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(url, ' ') FROM contact_urls WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(wallet_address, ' ') FROM contact_wallet_addresses WHERE contact_id = c.id), '') || ' ' ||
        COALESCE((SELECT GROUP_CONCAT(
            COALESCE(address1, '') || ' ' || COALESCE(address2, '') || ' ' || COALESCE(address3, '') || ' ' ||
            COALESCE(city, '') || ' ' || COALESCE(region, '') || ' ' || COALESCE(postal_code, '') || ' ' || COALESCE(country, ''), ' ')
         FROM contact_addresses WHERE contact_id = c.id), '')
    FROM contacts c WHERE c.id = OLD.contact_id;
END;
