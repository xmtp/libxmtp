# XMTP Crate

**⚠️ Experimental:** Early development stage, expect frequent changes and unresolved issues.

## State Machine

### Simplifying assumptions

- We use a pull-based approach for detecting if the installations in a conversation has changed - refreshing the installation list whenever the last refresh was more than THRESHOLD ago. We can add push-based mechanisms later.
- Every state update is a DB write. This allows us to resume on cold start.
- On cold start, we can scan the DB for UNINITIALIZED messages and payloads and resume sending them.
- Repeated sends of the same payload should be idempotent. When receiving a message, the receiving side will store the hash of the encrypted payload alongside the decrypted result. If a message is received with an id that already exists in the DB, it is ignored.
- We have ignored race conditions for now (as network requests may take different amounts of time). The receiver side should be tolerant of out-of-order payloads. If ordering is a must, it is possible to use multi-producer, single-consumer queues, or singleton threads for processMessages() and processPayloads() that run on an interval.

### Helper database models

`refresh_jobs`:

```sql
CREATE TABLE IF NOT EXISTS refresh_jobs (
    id TEXT PRIMARY KEY NOT NULL, # would be either `invite` or `messages`
    last_run BIGINT NOT NULL,
)
```

### States

```
Conversation:
    - UNINITIALIZED: No invites have been sent
    - INVITED: Invites have been sent
    - INVITE_RECEIVED: You have been invited to this conversation by another installation

User:
    - LAST_REFRESHED: The local timestamp at which an updated list of installations and pre-keys was requested for that user (and successfully received)

Installation:
    - UNINITIALIZED: There is no session state with that installation (no prekey messages were sent yet)
    - SESSION_CREATED: There is existing session state with that installation

Message:
    - UNPROCESSED: The message has not been encrypted yet
    - LOCALLY_COMMITTED: The outbound payloads have been constructed
    - RECEIVED: The message is inbound and was retrieved from the network

Outbound Payload:
    - PENDING: The payload has not been confirmed as sent yet
    - SERVER_ACKNOWLEDGED: The payload has been acknowledged by the server

Inbound invite:
    - PENDING: The payload has been downloaded from the network but has not been processed yet
    - PROCESSED: The invite has been successfully processed and the conversation has been created
    - DECRYPTION_FAILURE: The inner invite failed to decrypt
    - INVALID: The invite failed validation
```

### Creating a conversation

```
createConversation():
    Deterministically derive convo_id from peer_address
    If conversation with convo_id doesn't already exist in DB, insert it with conversation.state = UNINITIALIZED
    return conversation
```

### Sending a message in a conversation

```
send_message():
    Insert message into DB with message.state = UNPROCESSED and message.convo_id set
    process_messages() // Could be kicked off asynchronously or synchronously
    return success

process_messages():
    For each message in UNPROCESSED state, processing in order of timestamp:
        Fetch the conversation's users from the DB (including self)
        For each user:
            If user.last_refreshed is uninitialized or more than THRESHOLD ago:
                refresh_user_installations()    // Could be kicked off asynchronously or synchronously
                return  // refreshUserInstallations() will call back into processMessages() when ready
        Fetch the most recent sessions of all users from the DB
        For each session:
            // Build the plaintext payload
            If conversation.state == UNINITIALIZED:
                Construct the payload as an invite with the message attached
            Else if conversation.state == INVITED:
                Construct the payload as a message
            Use the existing session to encrypt the payload (hold it in memory)
        In a single transaction:
            Push all encrypted payloads to outbound_payloads table with outbound_payload.state = PENDING
            Commit all updated session states to the DB with installation.state = SESSION_CREATED
            Set message.state = LOCALLY_COMMITTED
            Set conversation.state = INVITED
            Delete all data from memory
    process_payloads()   // Could be kicked off asynchronously or synchronously
    return

refresh_user_installations(user):
    Fetch installations/contact bundles for the user from the network
    Fetch installations/contact bundles for the user from the DB

    For each installation from the DB:
            if is expired or revoked, delete it from the DB
    In a single transaction:
        For each new installation from the network:
            save installation to local cache
            create new session for new installation
        Set user.last_refreshed to NOW
    process_messages();  // Could be kicked off asynchronously or synchronously
    return

process_payloads():
    For each outbound payload in in UNINITIALIZED state, processing in order of sequential ID (possibly batched):
        Send the payload(s) to the server
        Once acknowledgement is received, set the payload to SERVER_ACKNOWLEDGED state and optionally delete. (We can turn off deletion for debugging purposes if needed)
```

### On cold start

```
init():
    // Could be kicked off asynchronously or synchronously
    process_messages()
    process_payloads()
    return
```

### Receiving invites

```plaintext
download_invites():
    Get the `refresh_jobs` record with an id of `invites`, and obtain a lock on the row:
        Store `now()` in memory to mark the job execution start
        Fetch all messages from invite topic with timestamp > refresh_job.last_run - PADDING_TIME # PADDING TIME accounts for eventual consistency of network. Maybe 30s.
        For each message in topic:
            Save (or ignore if already exists) raw message to inbound_invite table with status of PENDING
        Update `refresh_jobs` record last_run = current_timestamp

process_invites():
    For each inbound_invite in PENDING state:
        If the payload is malformed and the proto cannot be decoded:
            Set inbound_invite state to INVALID
            continue
        If an existing session exists with the `inviter.installation_id`:
            Decrypt the inner invite using the existing session
            If decryption fails:
                Set inbound_invite state to DECRYPTION_FAILURE
                continue
            Update session in the database
        else:
            Create a new inbound session with the inviter
            Decrypt the inner invite using the new session
            If decryption fails:
                Set inbound_invite state to DECRYPTION_FAILURE
                continue
            Persist the session to the database

        If invite validation fails:
            Set inbound_invite state to INVALID
            continue

        Fetch the existing conversation with convo_id derived from inner invite
        If conversation with convo_id doesn't already exist in DB:
            Insert conversation with state = INVITE_RECEIVED

        If invite has message attached:
            Insert message with state = RECEIVED and convo_id matching the record stored DB

        Set inbound_invite state to PROCESSED

update_conversations():
    download_invites()
    process_invites()
```

...

### Receiving a message

```plaintext
download_invites():
    Get the `refresh_jobs` record with an id of `invites`, and obtain a lock on the row:
        Store `now()` in memory to mark the job execution start
        Fetch all messages from invite topic with timestamp > refresh_job.last_run - PADDING_TIME # PADDING TIME accounts for eventual consistency of network. Maybe 30s.
        For each message in topic:
            Save (or ignore if already exists) raw message to inbound_invite table with status of PENDING
        Update `refresh_jobs` record last_run = current_timestamp

process_inbound_messages():
    For each inbound_message in PENDING state:
        If the payload is malformed and the proto cannot be decoded:
            Set inbound_invite state to INVALID
            continue

        If session from get_session:
            Decrypt ciphertext using session
            If decryption fails:
                Set inbound_message state to DECRYPTION_FAILURE
                continue
            Update/save session in the database

        If message validation fails:
            Set inbound_message state to INVALID
            continue

        persist message
        Set inbound_message state to PROCESSED

get_session(message):

    For each session with `session.installation_id` == `sender.installation_id`:  // Regardless of message type
        If the message can be decrypted:
            return session

    if message.type == Prekey:
        return new inbound session
    else:
        NoSession available -- message cannot be decrypted
```

...

### Receiving a ratchet message

...

### Receiving a decryption failure message

...
