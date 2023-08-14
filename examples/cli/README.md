# XLI

XLI is an demo XMTPv3 console client, which allows developers to send and receive messages via the command line. While potentially useful its primary purpose is to demonstrate how to make a client application using the Rust Apis.

## Running

### Register accounts

`./xli.sh --db user1.db3 register`
`./xli.sh --db user2.db3 register`

### Get wallet address

`./xli.sh --db user2.db3 info`

### Send message

`./xli.sh --db user1.db3 send <user2_address> "hello"`

### List conversations

`./xli.sh --db user1.db3 list-conversations`
