FROM rust:1-bullseye as builder
WORKDIR /code
COPY . .
COPY .git /.git
RUN cargo build --release --package xdbg

FROM debian:bullseye-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl1.1 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /code/target/release/xdbg /usr/local/bin/xdbg
ENV RUST_LOG=info
ENTRYPOINT ["xdbg"]
