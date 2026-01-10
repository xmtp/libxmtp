-- Drop FTS triggers first
DROP TRIGGER IF EXISTS contact_addresses_fts_delete;
DROP TRIGGER IF EXISTS contact_addresses_fts_update;
DROP TRIGGER IF EXISTS contact_addresses_fts_insert;
DROP TRIGGER IF EXISTS contact_aliases_fts_delete;
DROP TRIGGER IF EXISTS contact_aliases_fts_update;
DROP TRIGGER IF EXISTS contact_aliases_fts_insert;
DROP TRIGGER IF EXISTS contact_wallet_addresses_fts_delete;
DROP TRIGGER IF EXISTS contact_wallet_addresses_fts_update;
DROP TRIGGER IF EXISTS contact_wallet_addresses_fts_insert;
DROP TRIGGER IF EXISTS contact_urls_fts_delete;
DROP TRIGGER IF EXISTS contact_urls_fts_update;
DROP TRIGGER IF EXISTS contact_urls_fts_insert;
DROP TRIGGER IF EXISTS contact_emails_fts_delete;
DROP TRIGGER IF EXISTS contact_emails_fts_update;
DROP TRIGGER IF EXISTS contact_emails_fts_insert;
DROP TRIGGER IF EXISTS contact_phone_numbers_fts_delete;
DROP TRIGGER IF EXISTS contact_phone_numbers_fts_update;
DROP TRIGGER IF EXISTS contact_phone_numbers_fts_insert;
DROP TRIGGER IF EXISTS contacts_fts_delete;
DROP TRIGGER IF EXISTS contacts_fts_update;
DROP TRIGGER IF EXISTS contacts_fts_insert;

-- Drop FTS table
DROP TABLE IF EXISTS contacts_fts;

-- Drop view
DROP VIEW IF EXISTS contact_list;

-- Drop companion tables (due to foreign key constraints)
DROP TABLE IF EXISTS contact_addresses;
DROP TABLE IF EXISTS contact_aliases;
DROP TABLE IF EXISTS contact_wallet_addresses;
DROP TABLE IF EXISTS contact_urls;
DROP TABLE IF EXISTS contact_emails;
DROP TABLE IF EXISTS contact_phone_numbers;

-- Drop main contacts table
DROP TABLE IF EXISTS contacts;
