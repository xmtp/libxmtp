// @generated automatically by Diesel CLI.

/*
SIMPLIFYING ASSUMPTIONS:
- The installations in a conversation do not change after the conversation is initialized. Can sketch out plan for adding/removing installations from existing conversations later.
- We do not include message data on invites. Ideally we start including messages directly on invites later.
- Every state update is a DB write. This allows us to resume on cold start.
- On cold start, we can scan the DB for UNINITIALIZED messages and resume sending them.
- Every message and invite has a randomly generated id. This is used to deduplicate on the receiving side. If a message is received with an id that already exists in the DB, it is ignored (after advancing the ratchet state).
- We set state enum values as 0, 10, 20 etc. to allow for future additions to the enum without breaking the schema.

Creating a conversation:
    Deterministically derive convo_id from peer_address
    If conversation with convo_id doesn't already exist in DB, insert it with conversation.state = UNINITIALIZED

Sending a message in a conversation:
    Insert message into DB with message.state = UNINITIALIZED and message.convo_id set

For messages in UNINITIALIZED state:
    If conversation.state == UNINITIALIZED:
         For each user in the conversation (including self):
             If user.last_refreshed is uninitialized or more than THRESHOLD ago:
                 Fetch installations/contact bundles for the user
                 For each installation:
                     If it doesn't already exist in the DB, insert it with installation.state = UNINITIALIZED
                 Set user.last_refreshed to NOW
             For each installation of the user:
                 If installation.state == UNINITIALIZED:
                     Send invite as prekey message
                     Set installation state to PREKEY_MESSAGE_SENT
                 Else if installation.state == PREKEY_MESSAGE_SENT
                     Send invite as ratchet message
         Set conversation.state = INVITES_SENT
    If conversation.state == INVITES_SENT:
        For each user in the conversation (including self):
            For each installation of the user:
                installation.state must be PREKEY_MESSAGE_SENT:
                    Send message as ratchet message
        Set message.state = SENT

Receiving an invite:
...

Receiving a pre-key message:
...

Receiving a ratchet message:
...

Receiving a decryption failure message:
...
*/

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
