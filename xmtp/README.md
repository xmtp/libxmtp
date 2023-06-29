# XMTP Crate

**⚠️ Experimental:** Early development stage, expect frequent changes and unresolved issues.

## State Machine

### Simplifying assumptions

- We use a pull-based approach for detecting if the installations in a conversation has changed - refreshing the installation list whenever the last refresh was more than THRESHOLD ago. We can add push-based mechanisms later.
- We include message data directly on invites.
- Every state update is a DB write. This allows us to resume on cold start.
- On cold start, we can scan the DB for UNINITIALIZED messages and payloads and resume sending them.
- Repeated sends of the same payload should be idempotent. When receiving a message or invite, the receiving side will store the hash of the encrypted payload alongside the decrypted result. If a message is received with an id that already exists in the DB, it is ignored.

### Creating a conversation

```
Deterministically derive convo_id from peer_address
If conversation with convo_id doesn't already exist in DB, insert it with conversation.state = UNINITIALIZED
```

### Sending a message in a conversation

```
Insert message into DB with message.state = UNINITIALIZED and message.convo_id set

For each message in UNINITIALIZED state, processing in order of timestamp:
    For each user in the conversation (including self):
        If user.last_refreshed is uninitialized or more than THRESHOLD ago:
            Fetch installations/contact bundles for the user from the network
            Fetch installations/contact bundles for the user from the DB
            For each installation from the DB:
                If it doesn't exist in the network contact bundles or is expired or revoked, delete it from the DB
            For each installation from the network:
                If it doesn't exist in the DB, insert it with installation.state = UNINITIALIZED
            Set user.last_refreshed to NOW
        For each installation of the user:
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
        Push all encrypted payloads to outbound_payloads table with outbound_payload.state = UNINITIALIZED
        Commit all updated session states to the DB with installation.state = SESSION_CREATED
        Set message.state = COMMITTED
        Set conversation.state = INVITED
        Delete all data from memory


For each outbound payload in in UNINITIALIZED state, processing in order of sequential ID (possibly batched):
    Send the payload(s) to the server
    Once acknowledgement is received, set the payload to SENT state and optionally delete. (We can turn off deletion for debugging purposes if needed)
```

### Receiving an invite

...

### Receiving a pre-key message

...

### Receiving a ratchet message

...

### Receiving a decryption failure message

...
