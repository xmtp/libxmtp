# XMTP Crate

**⚠️ Experimental:** Early development stage, expect frequent changes and unresolved issues.

## State Machine

### Simplifying assumptions

- The installations in a conversation do not change after the conversation is initialized. Can sketch out plan for adding/removing installations from existing conversations later.
- We do not include message data on invites. Ideally we start including messages directly on invites later.
- Every state update is a DB write. This allows us to resume on cold start.
- On cold start, we can scan the DB for UNINITIALIZED messages and resume sending them.
- Repeated sends of the same payload should be idempotent. When receiving a message or invite, the receiving side will store the hash of the encrypted payload alongside the decrypted result. If a message is received with an id that already exists in the DB, it is ignored.

### Creating a conversation

```
Deterministically derive convo_id from peer_address
If conversation with convo_id doesn't already exist in DB, insert it with conversation.state = UNINITIALIZED
```

### Sending a message in a conversation

```
Insert message into DB with message.state = UNINITIALIZED and message.convo_id set

For messages in UNINITIALIZED state:

    // Send out invites first if the conversation is uninitialized
    If conversation.state == UNINITIALIZED:
         For each user in the conversation (including self):
             If user.last_refreshed is uninitialized or more than THRESHOLD ago:
                 Fetch installations/contact bundles for the user
                 For each installation:
                     If it doesn't already exist in the DB, insert it with installation.state = UNINITIALIZED
                 Set user.last_refreshed to NOW
             For each installation of the user:
                 If installation.state == UNINITIALIZED:
                     Create an outbound session
                     Set installation state to SESSION_CREATED
                 Send the invite on the existing session
         Set conversation.state = INVITES_SENT

    // Send out the message once the conversation is initialized
    If conversation.state == INVITES_SENT:
        For each user in the conversation (including self):
            For each installation of the user:
                installation.state must be SESSION_CREATED:
                    Send the message on the existing session
        Set message.state = SENT
```

### Receiving an invite

...

### Receiving a pre-key message

...

### Receiving a ratchet message

...

### Receiving a decryption failure message

...
