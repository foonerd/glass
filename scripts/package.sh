#!/bin/bash
# Assemble the Volumio plugin zip: the plugin directory, the display binaries
# from bin/<arch>, the ALSA scope from the peppyalsa builds, and the node
# modules installed through docker. Output: dist/glass-<version>.zip.
set -e
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
PEPPYALSA=${PEPPYALSA:-$ROOT/../peppy_builds/peppyalsa-builds/out}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
STAGE=$ROOT/dist/stage/glass
rm -rf "$ROOT/dist/stage"
mkdir -p "$STAGE" "$ROOT/dist"
cp -r "$ROOT/plugin/." "$STAGE/"
rm -f "$STAGE/README.md"

# The plugin version follows the workspace version.
sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$STAGE/package.json"

# Display binaries, the tap's dump tool and the tap library, one set per
# Volumio architecture name.
for arch in arm armv7 armv8 x64; do
  if [ -x "$ROOT/bin/$arch/glass" ]; then
    mkdir -p "$STAGE/bin/$arch" "$STAGE/lib/$arch"
    cp "$ROOT/bin/$arch/glass" "$STAGE/bin/$arch/glass"
    [ -x "$ROOT/bin/$arch/tapdump" ] && cp "$ROOT/bin/$arch/tapdump" "$STAGE/bin/$arch/tapdump"
    [ -f "$ROOT/lib/$arch/libglasstap.so" ] && cp "$ROOT/lib/$arch/libglasstap.so" "$STAGE/lib/$arch/libglasstap.so"
  fi
done

# The ALSA scope and its client, by the Debian architecture names the builds use.
copy_alsa() {
  local arch=$1 deb=$2
  if [ -f "$PEPPYALSA/$deb/libpeppyalsa.so" ]; then
    mkdir -p "$STAGE/lib/$arch" "$STAGE/bin/$arch"
    cp "$PEPPYALSA/$deb/libpeppyalsa.so" "$STAGE/lib/$arch/libpeppyalsa.so"
    [ -f "$PEPPYALSA/$deb/peppyalsa-client" ] && cp "$PEPPYALSA/$deb/peppyalsa-client" "$STAGE/bin/$arch/peppyalsa-client"
  else
    echo "package: no peppyalsa build for $arch ($deb)" >&2
  fi
}
copy_alsa arm armhf
copy_alsa armv7 armhf
copy_alsa armv8 arm64
copy_alsa x64 amd64

# Node modules, installed in a container so the host needs no node.
docker run --rm -v "$STAGE:/plugin" -w /plugin node:20-slim sh -c "npm install --omit=dev --no-audit --no-fund --loglevel=error >/dev/null && chown -R $(id -u):$(id -g) /plugin/node_modules /plugin/package-lock.json 2>/dev/null || true"
rm -f "$STAGE/package-lock.json"

( cd "$STAGE" && rm -f "$ROOT/dist/glass-$VERSION.zip" && zip -qr "$ROOT/dist/glass-$VERSION.zip" . )
rm -rf "$ROOT/dist/stage"
ls -la "$ROOT/dist/glass-$VERSION.zip"
