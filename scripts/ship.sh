#!/bin/bash
# Build the glass binary for each Volumio architecture and copy it into
# bin/<arch>/glass. Devices run that file. They do not run cargo.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
unset CARGO_TARGET_DIR

fetch_sdl() {
  local deb_arch=$1
  local dest=$2
  local deb="target/sysroot/${deb_arch}.deb"
  mkdir -p "target/sysroot/$deb_arch" "$dest"
  if [ ! -e "$dest/libSDL2.so" ]; then
    curl -fsSL -o "$deb" \
      "http://deb.debian.org/debian/pool/main/libs/libsdl2/libsdl2-2.0-0_2.26.5+dfsg-1_${deb_arch}.deb"
    dpkg-deb -x "$deb" "target/sysroot/$deb_arch"
    local so
    so=$(find "target/sysroot/$deb_arch" -name 'libSDL2-2.0.so.0' | head -n1)
    ln -sfn "$(realpath "$so")" "$dest/libSDL2.so"
  fi
}

fetch_sdl armhf target/sysroot/link-armhf
fetch_sdl arm64 target/sysroot/link-arm64
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-armhf -C link-arg=-Wl,--allow-shlib-undefined"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-arm64 -C link-arg=-Wl,--allow-shlib-undefined"

ship() {
  local triple=$1
  local arch=$2
  local stripper=$3
  echo "ship: $arch ($triple)"
  cargo build --release --locked --target "$triple" --bin glass
  install -D -m 755 "target/$triple/release/glass" "bin/$arch/glass"
  "$stripper" "bin/$arch/glass"
}

ship x86_64-unknown-linux-gnu x64 strip
ship armv7-unknown-linux-gnueabihf armv7 arm-linux-gnueabihf-strip
install -D -m 755 bin/armv7/glass bin/arm/glass
ship aarch64-unknown-linux-gnu armv8 aarch64-linux-gnu-strip

echo "ship: payload"
file bin/arm/glass bin/armv7/glass bin/armv8/glass bin/x64/glass
