# Sequence Diagrams

The sequence diagrams stored here are for documenting LibXMTP's group chat implementation using MLS.  They are part of a work in progress but we are putting them here for transparency and to keep them current with the implementation.

The diagrams represent the creation of a group chat between Alice, Bob, and Charlie, our implmentation of [Figure 2](https://messaginglayersecurity.rocks/mls-architecture/draft-ietf-mls-architecture.html#fig-group-formation-example) from [The Messaging Layer Security (MLS) Architecture](https://messaginglayersecurity.rocks/mls-architecture/draft-ietf-mls-architecture.html) spec. 

Note: calls into LibXMTP with the `conversations.` prefix use the [Conversations](https://github.com/xmtp/libxmtp/blob/204b35a337daf2a9f2ed0cb20199e254d0a7493a/bindings_ffi/src/mls.rs#L188) protocol, and calls with a `group.` prefix use the [Group](https://github.com/xmtp/libxmtp/blob/204b35a337daf2a9f2ed0cb20199e254d0a7493a/bindings_ffi/src/mls.rs#L315) protocol.

* *form-group.mermaid* - Covers Steps 1-4 of forming a group.  In LibXMTP, steps 1 and 2 happen at the same time, and steps 3 and 4 can also be consolidated by calling `newGroup()` with multiple participants.
* *send-recieve.mermaid* - Covers sending and receiving messages to the newly formed group.
* *add-remove.mermaid* - Covers adding and removing group members.
* *sync-installations.mermaid* - Covers how to find out if group members have added/removed an installation, and how to respond.

## Forming a Group

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    participant Charlie
    participant LibXMTP
    participant Node

    Note over Alice,Charlie: These calls are coming from higher-level<br/> SDKs on behalf of users
    Note over Alice,Node: Step 1 (Account Creation) & 2 (Initial Keying Material) of MLS group creation combined
    Alice->>+LibXMTP: create_client(encryption_key, account_address)
    LibXMTP-->>-Alice: client
    Alice->>+LibXMTP: client.text_to_sign()
    LibXMTP-->>-Alice: text to be signed for register_identity
    Alice->>LibXMTP: client.register_identity(recoverable_wallet_signature)
    LibXMTP->>+Node: register_installation(key_package:Alice)
    Node-->>-LibXMTP: installation_key:Alice
    Bob->>+LibXMTP: create_client(encryption_key, account_address)
    LibXMTP-->>-Bob: client
    Bob->>+LibXMTP: client.text_to_sign()
    LibXMTP-->>-Bob: text to be signed for register_identity
    Bob->>LibXMTP: client.register_identity(recoverable_wallet_signature)    
    LibXMTP->>+Node: register_installation(key_package:Bob)
    Node-->>-LibXMTP: installation_key:Bob
    Charlie->>+LibXMTP: create_client(encryption_key, account_address)
    LibXMTP-->>-Charlie: client
    Charlie->>+LibXMTP: client.text_to_sign()
    LibXMTP-->>-Charlie: text to be signed for register_identity
    Charlie->>LibXMTP: client.register_identity(recoverable_wallet_signature)  
    LibXMTP->>+Node: register_installation(key_package:Charlie)
    Node-->>-LibXMTP: installation_key:Charlie 

    Note over Alice,Node: Step 3 (Adding Bob) & 4 (Adding Charlie) of MLS group creation
    Alice->>LibXMTP: conversations.create_group(Bob, Charlie)
    LibXMTP->>+Node: get-identity-updates(Bob)
    Node-->>-LibXMTP: installation_key:Bob + credential_identity:Bob
    LibXMTP->>+Node: get-identity-updates(Charlie)
    Node-->>-LibXMTP: installation_key:Charlie + credential_identity:Charlie   
    LibXMTP->>Node: fetch-key-packages(installation_keys: Bob + Charlie)
    Node-->>LibXMTP: KeyPackages(Bob+Charlie) 
    LibXMTP->>Node: send-welcome-messages(KeyPackages:Bob + Charlie)
    Bob->>+LibXMTP: conversations.sync()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Bob)
    Node-->>-LibXMTP: WelcomeMessages()
    Bob->>+LibXMTP: conversations.list()
    LibXMTP-->>-Bob: List of groups including new group  
    Charlie->>+LibXMTP: conversations.sync()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Charlie)
    Node-->>-LibXMTP: WelcomeMessages()
    Charlie->>+LibXMTP: conversations.list()
    LibXMTP-->>-Charlie: List of groups including new group  
```

## Send and Receive Messages

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    participant LibXMTP
    participant Node

    Note left of Alice: Send Message
    Alice->>LibXMTP: group.send("Hello, group!")
    LibXMTP->>Node: send-group-messages(SEND_MESSAGE:"Hello, group!")

    Note left of Alice: Receive Message
    Bob->>LibXMTP: group.sync()
    LibXMTP->>Node: query-group-messages(group_id)
    Node-->>LibXMTP: "Hello, group!"
    Bob->>LibXMTP: group.find_messages()
    LibXMTP->>Bob: "Hello, group!"
```

## Add and Remove Group Members

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    participant Charlie
    participant LibXMTP
    participant Node

    Note left of Alice: Remove Charlie

    Alice->>LibXMTP: group.remove_members(Charlie)
    LibXMTP->>Node: send-group-message(REMOVE_MEMBER:installation_key:Charlie)
    Alice->>+LibXMTP: group.sync()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: REMOVE_MEMBER:Charlie
    LibXMTP-->>-Alice: "Charlie has been removed from the group"
    Bob->>+LibXMTP: group.sync()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: REMOVE_MEMBER:Charlie
    LibXMTP-->>-Bob: "Charlie has been removed from the group"

    Note left of Alice: Add Charlie
    Alice->>LibXMTP: add_members(Charlie)
    LibXMTP->>+Node: get-identity-updates(Charlie)
    Node-->>-LibXMTP: installation_key:Charlie + credential_identity:Charlie  
    LibXMTP->>Node: send-group-message(ADD_MEMBER:installation_key:Charlie)
    Alice->>+LibXMTP: group.sync()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: ADD_MEMBER:Charlie
    LibXMTP-->>-Alice: "Charlie has been added to the group"
    Bob->>+LibXMTP: group.sync()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: ADD_MEMBER:Charlie
    LibXMTP-->>-Bob: "Charlie has been added to the group"    
    Charlie->>+LibXMTP: conversations.sync()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Charlie)
    Node-->>-LibXMTP: WelcomeMessages()
    LibXMTP-->>-Charlie: "Alice has added you to a group"   
```

## Sync Installations

```mermaid
sequenceDiagram
    participant LibXMTP
    participant Node

    Note left of LibXMTP: Sync installations
    LibXMTP->>Node: get-identity-updates(Alice, Bob, Charlie)
    Node-->>LibXMTP: Added and Revoked installation_keys
    LibXMTP->>Node: send-group-message(REMOVE_MEMBER:installation_key:revoked installation)
    LibXMTP->>Node: send-group-message(ADD_MEMBER:installation_key:new installation) 
```
