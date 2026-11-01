#!/bin/bash
# The guard on evotap's version: a change to the tap cannot go out under
# a number already released. Asks github.com/foonerd/evotap for the
# release of the version the tree carries, and where one exists, compares
# the tap's files with the Glass commit that release was built from (its
# notes name it).
#   prints "new"  and exits 0: no release of this version; the cut publishes it
#   prints "same" and exits 0: released, and the tap is unchanged since; nothing to publish
#   exits 1: released, and the tap changed since: the version must move
#   exits 2: GitHub could not be asked; the verdict is CI's
# GH_TOKEN, when set, authenticates the question (a workflow's token will do).
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
REPO=foonerd/evotap
PATHS="crates/tap bins/evotap evotap/README.md"
version=$("$ROOT/scripts/evotap-version.sh")

answer=$(curl -sS -m 20 -w '\n%{http_code}' ${GH_TOKEN:+-H "Authorization: Bearer $GH_TOKEN"} \
  -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/$REPO/releases/tags/v$version" 2>/dev/null) \
  || { echo "evotap-guard: github.com could not be asked for evotap v$version" >&2; exit 2; }
code=${answer##*$'\n'}
body=${answer%$'\n'*}
case "$code" in
  404) echo new; exit 0 ;;
  200) ;;
  *) echo "evotap-guard: github.com answered $code for evotap v$version" >&2; exit 2 ;;
esac

base=$(printf '%s' "$body" | grep -oE 'Built in Glass [0-9.]+ at [0-9a-f]{7,40}' | head -n1 | awk '{print $NF}')
if [ -z "$base" ]; then
  echo "evotap-guard: evotap v$version is released but its notes do not name the Glass commit it was built from" >&2
  exit 1
fi
if ! git -C "$ROOT" cat-file -e "$base^{commit}" 2>/dev/null; then
  git -C "$ROOT" fetch -q --depth=1 origin "$base" 2>/dev/null \
    || { echo "evotap-guard: the commit $base evotap v$version was built from is not in this clone" >&2; exit 2; }
fi
# shellcheck disable=SC2086
if git -C "$ROOT" diff --quiet "$base" HEAD -- $PATHS; then
  echo same; exit 0
fi
echo "evotap-guard: evotap v$version is released, built from Glass commit $base, and the tap changed since:" >&2
# shellcheck disable=SC2086
git -C "$ROOT" diff --stat "$base" HEAD -- $PATHS >&2
echo "evotap-guard: move the version in bins/evotap/Cargo.toml and crates/tap/Cargo.toml" >&2
exit 1
