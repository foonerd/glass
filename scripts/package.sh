#!/bin/bash
# Assemble the Volumio plugin zip: the plugin directory, the display binaries
# from bin/<arch>, the tap from lib/<arch>, and the node
# modules installed through docker. Output: dist/glass-<version>.zip.
set -e
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
STAGE=$ROOT/dist/stage/glass
rm -rf "$ROOT/dist/stage"
mkdir -p "$STAGE" "$ROOT/dist"
cp -r "$ROOT/plugin/." "$STAGE/"
rm -f "$STAGE/README.md"

# The plugin version follows the workspace version.
sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$STAGE/package.json"

# The build the zip carries, for the Status sheet: the commit and the time.
printf '{ "commit": "%s", "built": "%s" }\n' \
  "$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)" \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$STAGE/build.json"

# Display binaries, the tap's dump tool and the tap library, one set per
# Volumio architecture name.
for arch in arm armv7 armv8 x64; do
  if [ -x "$ROOT/bin/$arch/glass" ]; then
    mkdir -p "$STAGE/bin/$arch" "$STAGE/lib/$arch"
    cp "$ROOT/bin/$arch/glass" "$STAGE/bin/$arch/glass"
    [ -x "$ROOT/bin/$arch/tapdump" ] && cp "$ROOT/bin/$arch/tapdump" "$STAGE/bin/$arch/tapdump"
    [ -x "$ROOT/bin/$arch/glass-serve" ] && cp "$ROOT/bin/$arch/glass-serve" "$STAGE/bin/$arch/glass-serve"
    [ -f "$ROOT/lib/$arch/libglasstap.so" ] && cp "$ROOT/lib/$arch/libglasstap.so" "$STAGE/lib/$arch/libglasstap.so"
  fi
done

# The multi-script text fonts, from the glass_fonts repository at a pinned
# commit, checked by digest: 16 MB each, fetched at packaging rather than
# kept in this repository. PeppyFont-Italic and the DSEG7 set are in git.
FONTS_COMMIT=6693040
fetch_font() {
  local name=$1 sha=$2
  local file="$STAGE/fonts/$name"
  local cache="$ROOT/target/sysroot/fonts/$name"
  if [ ! -f "$cache" ] || ! echo "$sha  $cache" | sha256sum -c - >/dev/null 2>&1; then
    "$ROOT/scripts/fetch.sh" "https://raw.githubusercontent.com/foonerd/glass_fonts/$FONTS_COMMIT/fonts/$name" "$cache" "$sha"
  fi
  cp "$cache" "$file"
}
fetch_font PeppyFont-Light.ttf 5ac2a7127f4670b30d21416ffb37b3ef3322c6d26c870794f5a736e6e5fd70ed
fetch_font PeppyFont-Regular.ttf 73f59fa152e94379ae6ed1e3624008e1cd446b29cc1d0f09293105b3c8481a8e
fetch_font PeppyFont-Bold.ttf 86de026072952f4a9101b53029d90b9c6cb60e6af70d5751e0300dfa0c0a0d19

# Node modules: with npm on this machine, directly; otherwise in a container.
if command -v npm >/dev/null 2>&1; then
  ( cd "$STAGE" && npm install --omit=dev --no-audit --no-fund --loglevel=error >/dev/null )
else
  docker run --rm -v "$STAGE:/plugin" -w /plugin node:20-slim sh -c "npm install --omit=dev --no-audit --no-fund --loglevel=error >/dev/null && chown -R $(id -u):$(id -g) /plugin/node_modules /plugin/package-lock.json 2>/dev/null || true"
fi
rm -f "$STAGE/package-lock.json"

( cd "$STAGE" && rm -f "$ROOT/dist/glass-$VERSION.zip" && zip -qr "$ROOT/dist/glass-$VERSION.zip" . )
rm -rf "$ROOT/dist/stage"
ls -la "$ROOT/dist/glass-$VERSION.zip"
