#!/bin/bash
# Build the glass binary for each Volumio architecture and copy it into
# bin/<arch>/glass. Devices run that file. They do not run cargo.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
unset CARGO_TARGET_DIR

# The libraries the cross builds link against, from Debian bookworm's own
# packages, each held against the digest Debian's index gives for it.
# Each library is looked for at its own path with -e, which follows links: a
# restored CI cache keeps the links of the sysroot and drops the files behind
# them, and such a library counts as missing and is unpacked again.
fetch_sdl() {
  local deb_arch=$1
  local multiarch=$2
  local dest=$3
  local sha=$4
  local deb="target/sysroot/${deb_arch}.deb"
  local so="target/sysroot/$deb_arch/usr/lib/$multiarch/libSDL2-2.0.so.0"
  if [ ! -e "$so" ]; then
    mkdir -p "target/sysroot/$deb_arch"
    "$ROOT/scripts/fetch.sh" "http://deb.debian.org/debian/pool/main/libs/libsdl2/libsdl2-2.0-0_2.26.5+dfsg-1_${deb_arch}.deb" "$deb" "$sha"
    dpkg-deb -x "$deb" "target/sysroot/$deb_arch"
  fi
  mkdir -p "$dest"
  ln -sfn "$(realpath "$so")" "$dest/libSDL2.so"
}

fetch_sdl armhf arm-linux-gnueabihf target/sysroot/link-armhf b76abdb214e7213a1ea6542dd618c940d3b5601c14dcf88577db1d13d6a80ec7
fetch_sdl arm64 aarch64-linux-gnu target/sysroot/link-arm64 7d875b119108a240015e7739c7a0de402616975281bcd8169cb434326fa93a66

# The ALSA library of each target, for the tap's link step (its build
# script finds it under target/sysroot/<arch>).
fetch_asound() {
  local deb_arch=$1
  local multiarch=$2
  local sha=$3
  local deb="target/sysroot/asound-${deb_arch}.deb"
  local so="target/sysroot/$deb_arch/usr/lib/$multiarch/libasound.so.2"
  if [ ! -e "$so" ]; then
    mkdir -p "target/sysroot/$deb_arch"
    "$ROOT/scripts/fetch.sh" "http://deb.debian.org/debian/pool/main/a/alsa-lib/libasound2_1.2.8-1+b1_${deb_arch}.deb" "$deb" "$sha"
    dpkg-deb -x "$deb" "target/sysroot/$deb_arch"
  fi
}
fetch_asound armhf arm-linux-gnueabihf 97718ff5e552c9a7b9e77e8debbe219b0068a9ab5dcabedd0800c69dd8f0eb96
fetch_asound arm64 aarch64-linux-gnu 9fa889400fcee4b92c8f4a2fafbb7f2cd33444d9ec1665a71002ab67c06114bb
fetch_asound amd64 x86_64-linux-gnu 44c77b076a7b11ae99712439022d822245b1994c435da564ebd320bb676faf4c
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-armhf -C link-arg=-Wl,--allow-shlib-undefined"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-L native=$ROOT/target/sysroot/link-arm64 -C link-arg=-Wl,--allow-shlib-undefined"

ship() {
  local triple=$1
  local arch=$2
  local stripper=$3
  echo "ship: $arch ($triple)"
  cargo build --release --locked --target "$triple" --bin glass --bin tapdump --bin glass-serve -p glass -p tap -p glass-serve
  cargo build --release --locked --target "$triple" -p glasstap
  install -D -m 755 "target/$triple/release/glass" "bin/$arch/glass"
  install -D -m 755 "target/$triple/release/tapdump" "bin/$arch/tapdump"
  install -D -m 755 "target/$triple/release/glass-serve" "bin/$arch/glass-serve"
  install -D -m 644 "target/$triple/release/libglasstap.so" "lib/$arch/libglasstap.so"
  "$stripper" "bin/$arch/glass" "bin/$arch/tapdump" "bin/$arch/glass-serve" "lib/$arch/libglasstap.so"
}

ship x86_64-unknown-linux-gnu x64 strip
ship armv7-unknown-linux-gnueabihf armv7 arm-linux-gnueabihf-strip
install -D -m 755 bin/armv7/glass bin/arm/glass
install -D -m 755 bin/armv7/tapdump bin/arm/tapdump
install -D -m 755 bin/armv7/glass-serve bin/arm/glass-serve
install -D -m 644 lib/armv7/libglasstap.so lib/arm/libglasstap.so
ship aarch64-unknown-linux-gnu armv8 aarch64-linux-gnu-strip

# Volumio bookworm runs glibc 2.36 on every architecture. The cross
# toolchain's sysroot is newer, and a symbol versioned above 2.36 (even a
# weak one) makes the loader refuse the whole binary on the player.
# The browser module: the pipeline for the manager's Face tab, one file
# the plugin serves; it ships inside the plugin zip, not in bin/.
echo "ship: browser module"
cargo build --locked --profile face --target wasm32-unknown-unknown -p glass-face
install -D -m 644 target/wasm32-unknown-unknown/face/glass_face.wasm plugin/face/glass-face.wasm

echo "ship: glibc"
glibc_max() {
  readelf -W --dyn-syms "$1" | grep -o 'GLIBC_2\.[0-9]*' | sort -t. -k2,2n -u | tail -n1
}
for bin in bin/*/glass bin/*/tapdump bin/*/glass-serve lib/*/libglasstap.so; do
  version=$(glibc_max "$bin")
  case "$version" in
    GLIBC_2.3[7-9]|GLIBC_2.[4-9]*|GLIBC_2.[1-9][0-9][0-9])
      echo "ship: $bin needs $version, newer than Volumio's glibc 2.36" >&2
      readelf -W --dyn-syms "$bin" | grep "$version" >&2
      exit 1
      ;;
  esac
  echo "ship: $bin needs at most ${version:-no versioned glibc symbol}"
done

echo "ship: payload"
file bin/arm/glass bin/armv7/glass bin/armv8/glass bin/x64/glass lib/arm/libglasstap.so lib/armv8/libglasstap.so lib/x64/libglasstap.so
