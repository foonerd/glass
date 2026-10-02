# The machine the CI and release builds run on: Ubuntu 24.04 with the cross
# compilers, the libraries the host build links against, MinGW-w64 for the
# Windows link, Node, PowerShell for the installer check, and the pinned
# Rust toolchain with its targets. A job installs no system package when it
# runs, so it never waits for a package mirror.
#
# .github/workflows/builder.yml builds it and pushes it to
# ghcr.io/foonerd/glass-builder; the workflows name it by digest. On a
# developer's machine, from the repository's root:
#
#   docker build -t glass-builder -f scripts/builder/Dockerfile .
#   docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/glass" -w /glass glass-builder scripts/ship.sh
#
# The context is the repository's root, of which the build reads only
# rust-toolchain.toml (see Dockerfile.dockerignore).
FROM ubuntu:24.04

LABEL org.opencontainers.image.source=https://github.com/foonerd/glass
LABEL org.opencontainers.image.description="The machine Glass's CI and release builds run on"

RUN apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl file git xz-utils zip unzip python3 \
    gcc libc6-dev make \
    gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu libc6-dev-armhf-cross libc6-dev-arm64-cross \
    gcc-mingw-w64-x86-64 \
    libsdl2-dev libasound2-dev alsa-utils \
  && rm -rf /var/lib/apt/lists/*

# Node, for the plugin's tests, its lint and the modules the plugin zip carries.
ARG NODE_VERSION=20.20.2
ARG NODE_SHA256=df770b2a6f130ed8627c9782c988fda9669fa23898329a61a871e32f965e007d
RUN curl -fsSL -o /tmp/node.tar.xz "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz" \
  && echo "$NODE_SHA256  /tmp/node.tar.xz" | sha256sum -c - \
  && tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --no-same-owner \
  && rm /tmp/node.tar.xz /usr/local/CHANGELOG.md /usr/local/README.md /usr/local/LICENSE

# PowerShell, for the check of the Windows installer.
ARG PWSH_VERSION=7.4.20
ARG PWSH_SHA256=7cafae8e49d412afc49c5f4c19c30be84e67afee0a521d8ccce7bb14a1e2cec0
RUN curl -fsSL -o /tmp/powershell.deb "https://github.com/PowerShell/PowerShell/releases/download/v$PWSH_VERSION/powershell_$PWSH_VERSION-1.deb_amd64.deb" \
  && echo "$PWSH_SHA256  /tmp/powershell.deb" | sha256sum -c - \
  && apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends /tmp/powershell.deb \
  && rm -rf /var/lib/apt/lists/* /tmp/powershell.deb

# The Rust toolchain rust-toolchain.toml names, with its components and
# targets, and the Windows target for scripts/ship-windows.sh. Cargo's home
# is writable by any user, so the image also serves a developer's own uid.
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
COPY rust-toolchain.toml /tmp/toolchain/rust-toolchain.toml
RUN curl -fsSL -o /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init" \
  && echo "$RUSTUP_SHA256  /tmp/rustup-init" | sha256sum -c - \
  && chmod +x /tmp/rustup-init \
  && /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none \
  && cd /tmp/toolchain \
  && rustup toolchain install \
  && rustup target add x86_64-pc-windows-gnu \
  && rustup default "$(rustup show active-toolchain | cut -d' ' -f1)" \
  && rm -rf /tmp/rustup-init /tmp/toolchain \
  && chmod -R a+rwX "$CARGO_HOME"
