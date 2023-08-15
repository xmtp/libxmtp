# End-to-end test scenarios

These can be performed manually and/or automated. More scenarios will be added as they are implemented (XMTP v3 is a work in progress).

## How to read the tests

### Identifiers

- Accounts (in this case wallets) are identified by letters (A, B, C)
- Installations are identified by numbers (1, 2, 3)

For example, Client A1 is the first installation of account A, A2 is the second installation of account A, and B1 is the first installation of account B.

### Registering

Each 'register' step below creates an installation and associates it with the relevant account.

If performed against the CLI, multiple installations can be associated with the same wallet by generating a random number for the wallet and specifying it as a `--seed` param to the `register` command. Care must be taken to ensure a brand new seed is used for each invocation of the test so that previous test runs do not interfere.

### Verifying

Each 'verify' step below involves listing the conversations and messages for a given installation.

Each installation is expected to have the history of all messages sent or received from the moment it was registered onwards. Currently, messages sent before a specific installation was registered are not expected to be viewable on that installation.

Message history is expected to persist between cold starts and does not require a network connection to access.

## Test cases

### 1. Sending and receiving as installations are added

Clients are expected to detect and establish sessions with new installations as they register.

```
Register client A1 and B1
    Send a message from A1 to B
    Verify message was received in all clients
    Respond from B1 to A
    Verify message was received in all clients
Register client A2
    Send a message from A1 to B
    Verify message was received in all clients
Register client B2
    Send a message from A1 to B
    Verify message was received in all clients
Register client A3
    Send a message from A3 to B
    Verify message was received in all clients
```

### 2. Enumerate installations

Users are expected to be able to enumerate the installations that have been granted access to their account, as well as enumerate the installations that they are currently sending messages to.

```
Enumerate installations for A from A1 (3 installations)
Enumerate installations for B from A1 (2 installations)
```

### 3. Sending and receiving with varying network connections

Clients should handle scenarios with flaky connections or where the app is killed mid-send or receive.

```
Disable the connection on client A1
    Verify A1's message history is accessible offline
    Send a message from B1 to A
    Verify message is shown in B1 but not A1
    Send a message from A1 to B
    Verify message is shown in A1 but not B1
Enable the connection on client A1, and restart A1
    Verify both A1 and B1 show both messages
```
