# A Linux image that cross-compiles the display for Windows: the pinned
# Rust toolchain with the x86_64-pc-windows-gnu target, and MinGW-w64 for
# the link. scripts/ship-windows.sh runs inside it when the host has no
# MinGW of its own:
#
#   docker build -t glass-windows scripts/windows
#   docker run --rm -v "$PWD:/glass" -w /glass glass-windows scripts/ship-windows.sh
FROM rust:bookworm
RUN apt-get update \
  && apt-get install -y --no-install-recommends mingw-w64 zip \
  && rm -rf /var/lib/apt/lists/*
ARG RUST_VERSION=1.98.1
RUN rustup toolchain install "$RUST_VERSION" --profile minimal \
  && rustup target add --toolchain "$RUST_VERSION" x86_64-pc-windows-gnu
