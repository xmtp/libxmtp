// @generated automatically by Diesel CLI.

// This is a generated file - changes here are made for illustrative purposes.
// We can put up separate PR's with the actual code changes required to generate these changes.
// This file will not be landed as part of this PR.
use diesel::sql_types::Date;

// We set state enum values as 0, 10, 20 etc. to allow for future additions to the enum without breaking the schema.
enum ConversationState {
    Uninitialized = 0,
    Invited = 10,
}

struct UserState {
    last_refreshed: Date,
}

enum InstallationState {
    Uninitialized = 0,
    SessionCreated = 10,
}

enum MessageState {
    Uninitialized = 0,
    LocallyCommitted = 10,
}

enum OutboundPayloadState {
    Uninitialized = 0,
    Sent = 10,
}

diesel::table! {
    accounts (id) {
        id -> Integer,
        created_at -> BigInt,
        serialized_key -> Binary,
    }
}

// peer_address can be deterministically derived from convo_id (and vice versa)
diesel::table! {
    conversations (convo_id) {
        convo_id -> Text,
        peer_address -> Text, // links to users table
        created_at -> BigInt,
        state -> Integer, // ConversationState
    }
}

diesel::table! {
    messages (id) {
        id -> Text,
        created_at -> BigInt,
        convo_id -> Text,   // links to conversations table
        addr_from -> Text,
        content -> Binary,
        state -> Integer, // MessageState
    }
}

diesel::table! {
    users (user_address) {
        user_address -> Text,
        created_at -> BigInt,
        last_refreshed -> Date,
    }
}

// Invariant: only one session per peer installation
diesel::table! {
    installations (installation_address) {
        installation_address -> Text,
        created_at -> BigInt,
        contact_bundle -> Binary,
        user_address -> Text,   // links to users table
        session_id -> Text, // nullable - is null when installation.state is UNINITIALIZED
        vmac_session_data -> Binary, // nullable - is null when installation.state is UNINITIALIZED
        state -> Integer, // InstallationState
    }
}

diesel::table! {
    outbound_payloads (sequential_id) {
        sequential_id -> Integer,
        payload -> Binary,
        state -> Integer, // OutboundPayloadState
    }
}

diesel::allow_tables_to_appear_in_same_query!(accounts, messages,);
