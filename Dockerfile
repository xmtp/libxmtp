FROM ghcr.io/xmtp/rust:latest
USER xmtp

RUN ~xmtp/.cargo/bin/rustup toolchain install stable 
RUN ~xmtp/.cargo/bin/rustup component add rustfmt
RUN ~xmtp/.cargo/bin/rustup component add clippy

WORKDIR /workspaces/libxmtp
COPY --chown=xmtp:xmtp . .

ENV PATH=~xmtp/.cargo/bin:$PATH
ENV USER=xmtp

RUN ~xmtp/.cargo/bin/cargo check
RUN ~xmtp/.cargo/bin/cargo fmt --check
RUN ~xmtp/.cargo/bin/cargo clippy --all-features --no-deps
RUN ~xmtp/.cargo/bin/cargo clippy --all-features --no-deps --manifest-path xmtp/Cargo.toml
# some tests are setup as integration tests 👀 xmtp_mls
RUN for crate in xmtp xmtp_api_grpc xmtp_api_grpc_gateway xmtp_cryptography xmtp_proto xmtp_v2; do cd ${crate}; ~xmtp/.cargo/bin/cargo test; done

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
