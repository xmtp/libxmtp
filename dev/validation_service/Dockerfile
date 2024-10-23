FROM rust:1-bullseye as builder
WORKDIR /code
COPY . .
COPY .git /.git
RUN cargo build --release --features test-utils --package mls_validation_service

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y sqlite3 curl
COPY --from=builder /code/target/release/mls-validation-service /usr/local/bin/mls-validation-service
ENV RUST_LOG=info
ENTRYPOINT ["mls-validation-service"]
