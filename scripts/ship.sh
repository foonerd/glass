#!/bin/bash
# Build the glass binary for each Volumio architecture and copy it into
# bin/<arch>/glass. Devices run that file. They do not run cargo.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
unset CARGO_TARGET_DIR

# Each library is looked for at its own path with -e, which follows links: a
# restored CI cache keeps the links of the sysroot and drops the files behind
# them, and such a library counts as missing and is unpacked again.
fetch_sdl() {
  local deb_arch=$1
  local multiarch=$2
  local dest=$3
  local deb="target/sysroot/${deb_arch}.deb"
  local so="target/sysroot/$deb_arch/usr/lib/$multiarch/libSDL2-2.0.so.0"
  if [ ! -e "$so" ]; then
    mkdir -p "target/sysroot/$deb_arch"
    curl -fsSL -o "$deb" \
      "http://deb.debian.org/debian/pool/main/libs/libsdl2/libsdl2-2.0-0_2.26.5+dfsg-1_${deb_arch}.deb"
    dpkg-deb -x "$deb" "target/sysroot/$deb_arch"
  fi
  mkdir -p "$dest"
  ln -sfn "$(realpath "$so")" "$dest/libSDL2.so"
}

fetch_sdl armhf arm-linux-gnueabihf target/sysroot/link-armhf
fetch_sdl arm64 aarch64-linux-gnu target/sysroot/link-arm64

# The ALSA library of each target, for the tap's link step (its build
# script finds it under target/sysroot/<arch>).
fetch_asound() {
  local deb_arch=$1
  local multiarch=$2
  local deb="target/sysroot/asound-${deb_arch}.deb"
  local so="target/sysroot/$deb_arch/usr/lib/$multiarch/libasound.so.2"
  if [ ! -e "$so" ]; then
    mkdir -p "target/sysroot/$deb_arch"
    curl -fsSL -o "$deb" \
      "http://deb.debian.org/debian/pool/main/a/alsa-lib/libasound2_1.2.8-1+b1_${deb_arch}.deb"
    dpkg-deb -x "$deb" "target/sysroot/$deb_arch"
  fi
}
fetch_asound armhf arm-linux-gnueabihf
fetch_asound arm64 aarch64-linux-gnu
fetch_asound amd64 x86_64-linux-gnu
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-armhf -C link-arg=-Wl,--allow-shlib-undefined"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-arm64 -C link-arg=-Wl,--allow-shlib-undefined"

ship() {
  local triple=$1
  local arch=$2
  local stripper=$3
  echo "ship: $arch ($triple)"
  cargo build --release --locked --target "$triple" --bin glass --bin tapdump -p glass -p tap
  cargo build --release --locked --target "$triple" -p glasstap
  install -D -m 755 "target/$triple/release/glass" "bin/$arch/glass"
  install -D -m 755 "target/$triple/release/tapdump" "bin/$arch/tapdump"
  install -D -m 644 "target/$triple/release/libglasstap.so" "lib/$arch/libglasstap.so"
  "$stripper" "bin/$arch/glass" "bin/$arch/tapdump" "lib/$arch/libglasstap.so"
}

ship x86_64-unknown-linux-gnu x64 strip
ship armv7-unknown-linux-gnueabihf armv7 arm-linux-gnueabihf-strip
install -D -m 755 bin/armv7/glass bin/arm/glass
install -D -m 755 bin/armv7/tapdump bin/arm/tapdump
install -D -m 644 lib/armv7/libglasstap.so lib/arm/libglasstap.so
ship aarch64-unknown-linux-gnu armv8 aarch64-linux-gnu-strip

echo "ship: payload"
file bin/arm/glass bin/armv7/glass bin/armv8/glass bin/x64/glass lib/arm/libglasstap.so lib/armv8/libglasstap.so lib/x64/libglasstap.so
