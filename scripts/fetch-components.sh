#!/bin/bash
# The components the plugin ships but does not build, each from its own GitHub release
# at the version its lock file names (components/<name>.lock), one archive per
# architecture. Nothing is unpacked before it is verified: the release's SHA256SUMS
# against its Ed25519 signature with the evo release key the plugin carries
# (keys/release-signing.pub), each archive against the sums, and each archive's own
# MANIFEST against its signature and its files. Unpacked under
# target/components/<name>/<arch>/, where package.sh looks. The same fetch Outcast
# uses for evotap and evopeg.
#
#   scripts/fetch-components.sh                 every component, every architecture
#   scripts/fetch-components.sh outcast-lite    one component
set -euo pipefail
ROOT=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
ARCHES="arm armv7 armv8 x64"
PUB="$ROOT/keys/release-signing.pub"

fetch() { # url, file
  curl -fsSL --retry 3 --retry-delay 5 -o "$2" "$1" || { echo "fetch-components: $1 did not arrive" >&2; exit 1; }
}

one() {
  local lock=$1 name version repository base dir arch top
  name=$(sed -n 's/^NAME=//p' "$lock")
  version=$(sed -n 's/^VERSION=//p' "$lock")
  repository=$(sed -n 's/^REPOSITORY=//p' "$lock")
  [[ -n "$name" && -n "$version" && -n "$repository" ]] || { echo "fetch-components: $lock needs NAME, VERSION and REPOSITORY" >&2; exit 1; }
  base="https://github.com/$repository/releases/download/v$version"
  dir="$ROOT/target/components/$name"
  if [[ -f "$dir/VERSION" && "$(cat "$dir/VERSION")" == "$version" ]]; then
    echo "fetch-components: $name $version is here already"
    return 0
  fi
  rm -rf "$dir" && mkdir -p "$dir/download"
  echo "fetch-components: $name $version from $base"
  fetch "$base/SHA256SUMS" "$dir/download/SHA256SUMS"
  fetch "$base/SHA256SUMS.sig" "$dir/download/SHA256SUMS.sig"
  openssl pkeyutl -verify -pubin -inkey "$PUB" -rawin -in "$dir/download/SHA256SUMS" -sigfile "$dir/download/SHA256SUMS.sig" > /dev/null \
    || { echo "fetch-components: the SHA256SUMS of $name $version is not signed with the evo release key" >&2; exit 1; }
  for arch in $ARCHES; do
    top="$name-$version-$arch"
    fetch "$base/$top.tar.gz" "$dir/download/$top.tar.gz"
    (cd "$dir/download" && grep -F " $top.tar.gz" SHA256SUMS | sha256sum -c --quiet -) \
      || { echo "fetch-components: $top.tar.gz is not the file the signed sums name" >&2; exit 1; }
    mkdir -p "$dir/$arch"
    tar -C "$dir/$arch" -xzf "$dir/download/$top.tar.gz"
    [[ -f "$dir/$arch/MANIFEST" && -f "$dir/$arch/MANIFEST.sig" ]] || { echo "fetch-components: $top.tar.gz carries no MANIFEST" >&2; exit 1; }
    openssl pkeyutl -verify -pubin -inkey "$PUB" -rawin -in "$dir/$arch/MANIFEST" -sigfile "$dir/$arch/MANIFEST.sig" > /dev/null \
      || { echo "fetch-components: the MANIFEST of $top.tar.gz is not signed with the evo release key" >&2; exit 1; }
    (cd "$dir/$arch" && sha256sum -c --quiet MANIFEST) || { echo "fetch-components: $top.tar.gz does not match its MANIFEST" >&2; exit 1; }
    # The archive's top folder, by its name, so package.sh finds the files without it.
    [[ -d "$dir/$arch/$top" ]] || { echo "fetch-components: $top.tar.gz does not unpack to $top/" >&2; exit 1; }
    echo "  $top: verified, $(grep -c . "$dir/$arch/MANIFEST") files"
  done
  # A Windows build, where the lock names its asset (WINDOWS=windows-x64): a
  # zip, held to the same sums and the same manifest inside it.
  local windows
  windows=$(sed -n 's/^WINDOWS=//p' "$lock")
  if [[ -n "$windows" ]]; then
    top="$name-$version-$windows"
    fetch "$base/$top.zip" "$dir/download/$top.zip"
    (cd "$dir/download" && grep -F " $top.zip" SHA256SUMS | sha256sum -c --quiet -) \
      || { echo "fetch-components: $top.zip is not the file the signed sums name" >&2; exit 1; }
    mkdir -p "$dir/$windows"
    unzip -q -o "$dir/download/$top.zip" -d "$dir/$windows"
    [[ -f "$dir/$windows/MANIFEST" && -f "$dir/$windows/MANIFEST.sig" ]] || { echo "fetch-components: $top.zip carries no MANIFEST" >&2; exit 1; }
    openssl pkeyutl -verify -pubin -inkey "$PUB" -rawin -in "$dir/$windows/MANIFEST" -sigfile "$dir/$windows/MANIFEST.sig" > /dev/null \
      || { echo "fetch-components: the MANIFEST of $top.zip is not signed with the evo release key" >&2; exit 1; }
    (cd "$dir/$windows" && sha256sum -c --quiet MANIFEST) || { echo "fetch-components: $top.zip does not match its MANIFEST" >&2; exit 1; }
    [[ -d "$dir/$windows/$top" ]] || { echo "fetch-components: $top.zip does not unpack to $top/" >&2; exit 1; }
    echo "  $top: verified, $(grep -c . "$dir/$windows/MANIFEST") files"
  fi
  echo "$version" > "$dir/VERSION"
}

if [[ $# -gt 0 ]]; then
  for name in "$@"; do one "components/$name.lock"; done
else
  for lock in components/*.lock; do one "$lock"; done
fi
