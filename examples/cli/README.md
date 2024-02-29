# CLI

![Status](https://img.shields.io/badge/Project_status-Alpha-orange)

This is a demo XMTP MLS console client (CLI) that you can use to send and receive messages via the command line.

> **Important**  
> This software is in **alpha** status and ready for you to start experimenting with. Expect frequent changes as we add features and iterate based on feedback.

## Create a MLS group

Use the CLI to send a [double ratchet message](https://github.com/xmtp/libxmtp/blob/main/README.md#double-ratchet-messaging) between test wallets on the XMTP `dev` network.

1. Go to the `examples/cli` directory.

2. Create a sender wallet account (user1). Create an [XMTP identity](../../xmtp_mls/IDENTITY.md) and store it in the database. Grant the installation key bundle permission to message on behalf of the sender address. This will allow the CLI to message on behalf of the sender address.

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

5. Create a new group and take note of the group ID.

   ```bash
   ./xli.sh --db user1.db3 create-group
   ```

6. Add user 2 to the group

   ```bash
   ./xli.sh --db user1.db3 add-group-members $GROUP_ID --account-addresses $USER_2_ACCOUNT_ADDRESS
   ```

7. Send a message

   ```bash
   ./xli.sh --db user1.db3 send $GROUP_ID "hello world"
   ```

8. Have User 2 read the message

   ```bash
   ./xli.sh --db user2.db3 list-group-messages $GROUP_ID
   ```

If you want to run the CLI against localhost, go to the root directory and run `dev/up` to start a local server. Then run the CLI commands using the `--local` flag.

## Structured logging

All commands in the CLI can be run with the `--json` option enabled to turn on structured logging. Each command will have at least one entry with `"command_output": true` as a value. Log events will be written to `stdout`. If the program finishes executing without a `"command_output": true` you should assume that it has failed.

Example output:

```
./target/cli-client --json --db user1.db3 list-groups
{"level":30,"time":1708561011597,"msg":"Starting CLI Client...."}
{"level":30,"time":1708561011597,"msg":"List Groups"}
{"level":30,"time":1708561011597,"msg":"Using persistent storage: user1.db3 "}
{"level":30,"time":1708561011597,"msg":"Setting up DB connection pool"}
{"level":30,"time":1708561011599,"msg":"Running DB migrations"}
{"level":30,"time":1708561011600,"msg":"Migrations successful"}
{"level":30,"time":1708561011600,"msg":"Using dev network"}
{"level":30,"time":1708561011999,"msg":"Initializing identity"}
{"level":30,"time":1708561012155,"msg":"group members","command_output":true,"success":true,"members":["0x2501622cbc306e06a09260c6f1dc4e166c4d814b","0xe11ac9f2b3f3c8b54690ef8c4c8e15c41c251bc1","0x8e6df612589feabc9524d371a018120175cb3b4d"],"group_id":"b360839b3d2e15bb86c2dca227095c14"}
```
