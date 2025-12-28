-- Drop companion tables first (due to foreign key constraints)
DROP TABLE IF EXISTS contact_street_addresses;
DROP TABLE IF EXISTS contact_aliases;
DROP TABLE IF EXISTS contact_wallet_addresses;
DROP TABLE IF EXISTS contact_urls;
DROP TABLE IF EXISTS contact_emails;
DROP TABLE IF EXISTS contact_phone_numbers;

-- Drop main contacts table
DROP TABLE IF EXISTS contacts;
