# XLI

XLI is an demo XMTPv3 console client, which allows developers to send and receive messages via the command line. While potentially useful its primary purpose is to demonstrate how to make a client application using the Rust Apis.

## Running

Register accounts:
`RUST_LOG=info cargo run -- --db ~/user1.db3 reg -L`
`RUST_LOG=info cargo run -- --db ~/user2.db3 reg -L`

Get wallet address:
`RUST_LOG=info cargo run -- --db ~/user2.db3 info`

Send message:
`RUST_LOG=info cargo run -- --db ~/user1.db3 send <user2_address> "hello"`
