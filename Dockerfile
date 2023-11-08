FROM debian:stable-slim as go-builder
# defined from build kit
# DOCKER_BUILDKIT=1 docker build . -t ...
ARG TARGETARCH

FROM debian:stable-slim as builder
# defined from build kit
# DOCKER_BUILDKIT=1 docker build . -t ...
ARG TARGETARCH

RUN export DEBIAN_FRONTEND=noninteractive && \
    apt update && \
    apt install -y -q --no-install-recommends \
    git curl gnupg2 build-essential \
    linux-headers-${TARGETARCH} libc6-dev \ 
    openssl libssl-dev pkg-config \
    ca-certificates apt-transport-https \
    python3 && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --create-home -s /bin/bash xmtp
RUN usermod -a -G sudo xmtp
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

WORKDIR /rustup
## Rust
ADD https://sh.rustup.rs /rustup/rustup.sh
RUN chmod 755 /rustup/rustup.sh

ENV USER=xmtp
USER xmtp
RUN /rustup/rustup.sh -y --default-toolchain stable --profile minimal

ENV PATH=$PATH:~xmtp/.cargo/bin

FROM debian:stable-slim
ARG TARGETARCH

RUN export DEBIAN_FRONTEND=noninteractive && \
  apt update && \
  apt install -y -q --no-install-recommends \
    ca-certificates apt-transport-https \
    sudo ripgrep procps build-essential \
    python3 python3-pip python3-dev \
    git curl && \
  apt clean && \
  rm -rf /var/lib/apt/lists/*

RUN echo "building platform $(uname -m)"

RUN useradd --create-home -s /bin/bash xmtp
RUN usermod -a -G sudo xmtp
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers

## Node and NPM
RUN mkdir -p /usr/local/nvm
ENV NVM_DIR=/usr/local/nvm

ENV NODE_VERSION=v20.9.0

ADD https://raw.githubusercontent.com/creationix/nvm/master/install.sh /usr/local/etc/nvm/install.sh
RUN bash /usr/local/etc/nvm/install.sh && \
    bash -c ". $NVM_DIR/nvm.sh && nvm install $NODE_VERSION && nvm alias default $NODE_VERSION && nvm use default"

ENV NVM_NODE_PATH ${NVM_DIR}/versions/node/${NODE_VERSION}
ENV NODE_PATH ${NVM_NODE_PATH}/lib/node_modules
ENV PATH      ${NVM_NODE_PATH}/bin:$PATH

RUN npm install npm -g
RUN npm install yarn -g


## Rust from builder
COPY --chown=xmtp:xmtp --from=builder /home/xmtp/.cargo /home/xmtp/.cargo
COPY --chown=xmtp:xmtp --from=builder /home/xmtp/.rustup /home/xmtp/.rustup

USER xmtp

RUN ~xmtp/.cargo/bin/rustup toolchain install stable 
RUN ~xmtp/.cargo/bin/rustup component add rustfmt
RUN ~xmtp/.cargo/bin/rustup component add clippy

WORKDIR /workspaces/libxmtp
COPY --chown=xmtp:xmtp . .

ENV PATH=~xmtp/.cargo/bin:$PATH
ENV USER=xmtp

RUN ~xmtp/.cargo/bin/cargo check
RUN ~xmtp/.cargo/bin/cargo --version
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
