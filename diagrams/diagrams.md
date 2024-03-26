# Sequence Diagrams

The sequence diagrams stored here are for documenting LibXMTP's group chat implementation using MLS.  They are part of a work in progress but we are putting them here for transparency and to keep them current with the implementation.

The diagrams represent the creation of a group chat between Alice, Bob, and Charlie, our implmentation of [Figure 2](https://messaginglayersecurity.rocks/mls-architecture/draft-ietf-mls-architecture.html#fig-group-formation-example) from [The Messaging Layer Security (MLS) Architecture](https://messaginglayersecurity.rocks/mls-architecture/draft-ietf-mls-architecture.html) spec. 

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
    Alice->>LibXMTP: create_client(encryption_key, account_address)
    LibXMTP->>+Node: register_installation(key_package:Alice)
    Node-->>-LibXMTP: installation_key:Alice
    Bob->>LibXMTP: create_client(encryption_key, account_address)
    LibXMTP->>+Node: register_installation(key_package:Bob)
    Node-->>-LibXMTP: installation_key:Bob
    Charlie->>LibXMTP: create_client(encryption_key, account_address)
    LibXMTP->>+Node: register_installation(key_package:Charlie)
    Node-->>-LibXMTP: installation_key:Charlie 

    Note over Alice,Node: Step 3 (Adding Bob) & 4 (Adding Charlie) of MLS group creation
    Alice->>LibXMTP: newGroup(Bob, Charlie)
    LibXMTP->>+Node: get-identity-updates(Bob)
    Node-->>-LibXMTP: installation_key:Bob + credential_identity:Bob
    LibXMTP->>+Node: get-identity-updates(Charlie)
    Node-->>-LibXMTP: installation_key:Charlie + credential_identity:Charlie   
    LibXMTP->>Node: fetch-key-packages(installation_keys: Bob + Charlie)
    Node-->>LibXMTP: KeyPackages(Bob+Charlie) 
    LibXMTP->>Node: send-welcome-messages(KeyPackages:Bob + Charlie)
    Bob->>+LibXMTP: syncGroups()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Bob)
    Node-->>-LibXMTP: WelcomeMessages()
    LibXMTP-->>-Bob: "Alice has added you to a group"   
    Bob->>LibXMTP: rotate_key_packages()
    LibXMTP->>Node: upload-key-package()
    Charlie->>+LibXMTP: syncGroups()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Charlie)
    Node-->>-LibXMTP: WelcomeMessages()
    LibXMTP-->>-Charlie: "Alice has added you to a group"   
    Charlie->>LibXMTP: rotate_key_packages()
    LibXMTP->>Node: upload-key-package() 
```

## Send and Receive Messages

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    participant Charlie
    participant LibXMTP
    participant Node

    Note left of Alice: Send Message
    Alice->>LibXMTP: group.send("Hello, group!")
    LibXMTP->>Node: send-group-messages(SEND_MESSAGE:"Hello, group!")

    Note left of Alice: Receive Message
    Bob->>+LibXMTP: group.messages()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node-->>-LibXMTP: "Hello, group!"
    LibXMTP->>-Bob: "Hello, group!"
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

    Alice->>LibXMTP: group.removeMembers(Charlie)
    LibXMTP->>Node: send-group-message(REMOVE_MEMBER:installation_key:Charlie)
    Alice->>+LibXMTP: group.messages()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: REMOVE_MEMBER:Charlie
    LibXMTP-->>-Alice: "Charlie has been removed from the group"
    Bob->>+LibXMTP: group.messages()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: REMOVE_MEMBER:Charlie
    LibXMTP-->>-Bob: "Charlie has been removed from the group"

    Note left of Alice: Add Charlie
    Bob->>LibXMTP: addMembers(Charlie)
    LibXMTP->>+Node: get-identity-updates(Charlie)
    Node-->>-LibXMTP: installation_key:Charlie + credential_identity:Charlie  
    LibXMTP->>Node: send-group-message(ADD_MEMBER:installation_key:Charlie)
    Bob->>+LibXMTP: group.messages()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: ADD_MEMBER:Charlie
    LibXMTP-->>-Bob: "Charlie has been added to the group"    
    Alice->>+LibXMTP: group.messages()
    LibXMTP->>+Node: query-group-messages(group_id)
    Node->>-LibXMTP: ADD_MEMBER:Charlie
    LibXMTP-->>-Alice: "Charlie has been added to the group"  
    Charlie->>+LibXMTP: syncGroups()
    LibXMTP->>+Node: query-welcome-messages(installation_key:Charlie)
    Node-->>-LibXMTP: WelcomeMessages()
    LibXMTP-->>-Charlie: "Alice has added you to a group"   
    Charlie->>LibXMTP: rotate_key_packages()
    LibXMTP->>Node: upload-key-package()   
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
