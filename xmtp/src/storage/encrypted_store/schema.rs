// @generated automatically by Diesel CLI.

// This is a generated file - changes here are made for illustrative purposes.
// Once we agree on what the changeset should be, I'll update the PR with the
// actual code changes required to generate these changes.
use diesel::sql_types::Date;

enum ConversationState {
    Uninitialized = 0,
    InvitesSent = 10,
}

struct UserState {
    last_refreshed: Date,
}

enum InstallationState {
    Uninitialized = 0,
    PrekeyMessageSent = 10,
}

enum MessageState {
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
        id -> Integer,
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
// Hence session id is not needed
diesel::table! {
    installations (installation_id) {
        installation_id -> Text,
        created_at -> BigInt,
        contact_bundle -> Binary,
        user_address -> Text,   // links to users table
        vmac_session_data -> Binary, // nullable - is null when installation.state is UNINITIALIZED
        state -> Integer, // InstallationState
    }
}

diesel::allow_tables_to_appear_in_same_query!(accounts, messages,);
