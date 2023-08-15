# CLI

![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

This is a demo XMTP v3-alpha console client (CLI) that you can use to send and receive messages via the command line. Specifically, you can use it to try out [double ratchet messaging](https://github.com/xmtp/libxmtp/blob/main/README.md#double-ratchet-messaging) and [installation key bundles](https://github.com/xmtp/libxmtp/blob/main/README.md#installation-key-bundles) enabled by XMTP v3-alpha.

> **Important**  
> This software is in **alpha** status and ready for you to start experimenting with. Expect frequent changes as we add features and iterate based on feedback.

## Send a double ratchet message

Use the CLI to send a [double ratchet message](https://github.com/xmtp/libxmtp/blob/main/README.md#double-ratchet-messaging) between test wallets using a local XMTP node and database running in Docker Desktop.

1. In the root directory of this repo, run:

   ```bash
   dev/up
   ```

2. Change directories to `examples/cli`.

   ```bash
   cd examples/cli
   ```

3. Create a sender wallet account (user1). Create an [installation key bundle](https://github.com/xmtp/libxmtp/blob/main/README.md#installation-key-bundles) and store it in the database. Grant the installation key bundle permission to message on behalf of the sender address. This will allow the CLI to message on behalf of the sender address.

   ```bash
   ./xli.sh --db user1.db3 register
   ```

3. Likewise, create a recipient wallet account (user2) and an installation key bundle.

   ```bash
   ./xli.sh --db user2.db3 register
   ```

4. Get the recipient's wallet address.

   ```bash
   ./xli.sh --db user2.db3 info
   ```

5. Send a message into the conversation. The message is sent using [one session between each installation](https://github.com/xmtp/libxmtp/blob/main/README.md#installation-key-bundles) associated with the sender and recipient. The message is encrypted using a session key dedicated to this single message per the [double ratchet algorithm](https://github.com/xmtp/libxmtp/blob/main/README.md#double-ratchet-messaging).

   ```bash
   ./xli.sh --db user1.db3 send <user2_address> "hello"
   ```

6. List conversations.

   ```bash
   ./xli.sh --db user1.db3 list-conversations
   ```
