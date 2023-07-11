# XMTP Crate

**⚠️ Experimental:** Early development stage, expect frequent changes and unresolved issues.

## State Machine

### Simplifying assumptions

- We use a pull-based approach for detecting if the installations in a conversation has changed - refreshing the installation list whenever the last refresh was more than THRESHOLD ago. We can add push-based mechanisms later.
- We include message data directly on invites.
- Every state update is a DB write. This allows us to resume on cold start.
- On cold start, we can scan the DB for UNINITIALIZED messages and payloads and resume sending them.
- Repeated sends of the same payload should be idempotent. When receiving a message or invite, the receiving side will store the hash of the encrypted payload alongside the decrypted result. If a message is received with an id that already exists in the DB, it is ignored.
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
    - UNINITIALIZED: The message has not been encrypted yet
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
sendMessage():
    Insert message into DB with message.state = UNINITIALIZED and message.convo_id set
    processMessages() // Could be kicked off asynchronously or synchronously
    return success

processMessages():
    For each message in UNINITIALIZED state, processing in order of timestamp:
        Fetch the conversation's users from the DB (including self)
        For each user:
            If user.last_refreshed is uninitialized or more than THRESHOLD ago:
                refreshUserInstallations()    // Could be kicked off asynchronously or synchronously
                return  // refreshUserInstallations() will call back into processMessages() when ready
        Fetch the installations of all users from the DB
        For each installation:
            // Build the plaintext payload
            If conversation.state == UNINITIALIZED:
                Construct the payload as an invite with the message attached
            Else if conversation.state == INVITED:
                Construct the payload as a message
            // Encrypt the payload
            If installation.state == UNINITIALIZED:
                Create an outbound session (hold it in memory)
            Use the existing session to encrypt a payload containing an invite with the message attached (hold it in memory)
        In a single transaction:
            Push all encrypted payloads to outbound_payloads table with outbound_payload.state = PENDING
            Commit all updated session states to the DB with installation.state = SESSION_CREATED
            Set message.state = LOCALLY_COMMITTED
            Set conversation.state = INVITED
            Delete all data from memory
    processPayloads()   // Could be kicked off asynchronously or synchronously
    return

refreshUserInstallations(user):
    Fetch installations/contact bundles for the user from the network
    Fetch installations/contact bundles for the user from the DB
    In a single transaction:
        For each installation from the DB:
            If it doesn't exist in the network contact bundles or is expired or revoked, delete it from the DB
        For each installation from the network:
            If it doesn't exist in the DB, insert it with installation.state = UNINITIALIZED
        Set user.last_refreshed to NOW
    processMessages();  // Could be kicked off asynchronously or synchronously
    return

processPayloads():
    For each outbound payload in in UNINITIALIZED state, processing in order of sequential ID (possibly batched):
        Send the payload(s) to the server
        Once acknowledgement is received, set the payload to SERVER_ACKNOWLEDGED state and optionally delete. (We can turn off deletion for debugging purposes if needed)
```

### On cold start

```
init():
    // Could be kicked off asynchronously or synchronously
    processMessages()
    processPayloads()
    return
```

### Receiving invites

```plaintext
downloadInvites():
    Get the `refresh_jobs` record with an id of `invites`, and obtain a lock on the row:
        Store `now()` in memory to mark the job execution time
        Fetch all messages from invite topic with timestamp > refresh_job.last_run - PADDING_TIME # PADDING TIME accounts for eventual consistency of network. Maybe 30s.
        For each message in topic:
            Save (or ignore if already exists) raw message to inbound_invite table with status of PENDING
        Update `refresh_jobs` record `last_run = current_timestamp`

processInvites():
    For each inbound_invite in PENDING state:
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

updateConversations():
    downloadInvites()
    processInvites()
```

...

### Receiving a pre-key message

...

### Receiving a ratchet message

...

### Receiving a decryption failure message

...
