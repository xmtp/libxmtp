FROM rust:1-bullseye as builder
WORKDIR /code
COPY . .
COPY .git /.git
RUN cargo build --release --package xdbg

FROM debian:bullseye-slim
COPY --from=builder /code/target/release/xdbg /usr/local/bin/xdbg
ENV RUST_LOG=info
ENTRYPOINT ["xdbg"]
