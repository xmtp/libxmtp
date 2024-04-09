FROM ghcr.io/xmtp/rust:latest

RUN sudo apt update && sudo apt install -y pkg-config openssl

WORKDIR /workspaces/libxmtp
COPY --chown=xmtp:xmtp . .

ENV RUSTUP_PERMIT_COPY_RENAME "yes"

RUN cargo check
RUN cargo fmt --check
RUN cargo clippy --all-features --no-deps -- -Dwarnings
# some tests are setup as integration tests 👀 xmtp_mls
RUN for crate in xmtp_cryptography xmtp_proto xmtp_v2; do cd ${crate}; cargo test; done

LABEL org.label-schema.build-date=$BUILD_DATE \
    org.label-schema.name="rustdev" \
    org.label-schema.description="Rust Development Container" \
    org.label-schema.url="https://github.com/xmtp/libxmtp" \
    org.label-schema.vcs-ref=$VCS_REF \
    org.label-schema.vcs-url="git@github.com:xmtp/libxmtp.git" \
    org.label-schema.vendor="xmtp" \
    org.label-schema.version=$VERSION \
    org.label-schema.schema-version="1.0" \
    org.opencontainers.image.description="Rust Development Container"
