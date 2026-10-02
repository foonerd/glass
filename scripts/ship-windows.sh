#!/bin/bash
# Cross-compile the display for Windows (x86_64, MinGW-w64) and put
# glass.exe with the SDL2.dll it loads into bin/windows-x64/. Runs on Linux
# with mingw-w64 installed and the x86_64-pc-windows-gnu target added, or
# inside the builder image scripts/builder/Dockerfile describes:
#
#   docker build -t glass-builder -f scripts/builder/Dockerfile .
#   docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/glass" -w /glass glass-builder scripts/ship-windows.sh
#
# SDL2 comes from the MinGW development package of the SDL project, pinned
# here by version and checksum and kept under target/sysroot/windows.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}

SDL_VERSION=2.32.10
SDL_SHA256=83a5d74012311edc3c0d40ea6faecbe57ad692aa033fa5dc273cc937e3938ff2
SDL_DIR=$TARGET_DIR/sysroot/windows/SDL2-$SDL_VERSION/x86_64-w64-mingw32
TRIPLE=x86_64-pc-windows-gnu

command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || {
  echo "ship-windows: x86_64-w64-mingw32-gcc is missing (apt install mingw-w64, or use scripts/builder/Dockerfile)" >&2
  exit 1
}

if [ ! -e "$SDL_DIR/bin/SDL2.dll" ]; then
  mkdir -p "$TARGET_DIR/sysroot/windows"
  tarball=$TARGET_DIR/sysroot/windows/SDL2-devel-$SDL_VERSION-mingw.tar.gz
  curl -fsSL -o "$tarball" \
    "https://github.com/libsdl-org/SDL/releases/download/release-$SDL_VERSION/SDL2-devel-$SDL_VERSION-mingw.tar.gz"
  echo "$SDL_SHA256  $tarball" | sha256sum -c - >/dev/null
  tar -C "$TARGET_DIR/sysroot/windows" -xzf "$tarball"
fi

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
# The import library of SDL2.dll for the link; the GCC runtime linked in,
# so the executable asks Windows for nothing beyond its own libraries and
# SDL2.dll beside it.
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-L native=$SDL_DIR/lib -C link-arg=-static-libgcc"

echo "ship-windows: glass ($TRIPLE)"
cargo build --release --locked --target "$TRIPLE" -p glass --bin glass
install -D -m 755 "$TARGET_DIR/$TRIPLE/release/glass.exe" bin/windows-x64/glass.exe
install -D -m 644 "$SDL_DIR/bin/SDL2.dll" bin/windows-x64/SDL2.dll
x86_64-w64-mingw32-strip bin/windows-x64/glass.exe

# What the executable loads at start: Windows' own libraries and SDL2.dll.
echo "ship-windows: imports"
imports=$(x86_64-w64-mingw32-objdump -p bin/windows-x64/glass.exe | awk '/DLL Name:/ {print $3}' | sort -u)
echo "$imports" | sed 's/^/ship-windows:   /'
for dll in $imports; do
  case "$(echo "$dll" | tr '[:upper:]' '[:lower:]')" in
    sdl2.dll|kernel32.dll|user32.dll|ws2_32.dll|advapi32.dll|bcrypt.dll|bcryptprimitives.dll|ntdll.dll|msvcrt.dll|userenv.dll|shell32.dll|ole32.dll|crypt32.dll|secur32.dll|gdi32.dll|imm32.dll|winmm.dll|version.dll|setupapi.dll|oleaut32.dll|api-ms-win-*) ;;
    *) echo "ship-windows: glass.exe needs $dll, which Windows does not ship" >&2; exit 1 ;;
  esac
done
echo "ship-windows: payload"
ls -l bin/windows-x64/
