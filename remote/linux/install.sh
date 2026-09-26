#!/bin/sh
# Install Glass as a remote display for this user, from an unpacked release
# archive: the binary into ~/.local/bin, two desktop entries (the display,
# and its settings page) with an icon, and with --service a user service
# that starts the display with the session and keeps it running.
#
#   tar xzf glass-<version>-<arch>.tar.gz
#   glass-<version>-<arch>/remote/linux/install.sh [--service]
#
# PREFIX changes where things go (default ~/.local). SDL2 must be installed:
#   sudo apt install libsdl2-2.0-0
set -eu

HERE=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$HERE/../.." && pwd)
PREFIX=${PREFIX:-$HOME/.local}
SERVICE=no
for arg in "$@"; do
  case "$arg" in
    --service) SERVICE=yes ;;
    --help|-h) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "install.sh: unknown argument $arg" >&2; exit 2 ;;
  esac
done

case "$(uname -m)" in
  x86_64) ARCH=x64 ;;
  aarch64) ARCH=armv8 ;;
  armv7l|armv6l) ARCH=armv7 ;;
  *) echo "install.sh: no Glass build for $(uname -m)" >&2; exit 1 ;;
esac
BIN=$ROOT/bin/$ARCH/glass
if [ ! -x "$BIN" ]; then
  if [ -x "$ROOT/bin/arm/glass" ] && [ "$ARCH" = armv7 ]; then
    BIN=$ROOT/bin/arm/glass
  else
    echo "install.sh: no binary at $BIN (this archive is for another architecture)" >&2
    exit 1
  fi
fi

install -D -m 755 "$BIN" "$PREFIX/bin/glass"
install -D -m 644 "$HERE/glass-remote.svg" "$PREFIX/share/icons/hicolor/scalable/apps/glass-remote.svg"
mkdir -p "$PREFIX/share/applications"
for entry in glass-remote glass-remote-settings; do
  sed "s|@BIN@|$PREFIX/bin/glass|g" "$HERE/$entry.desktop" > "$PREFIX/share/applications/$entry.desktop"
done
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo "installed: $PREFIX/bin/glass"
echo "installed: $PREFIX/share/applications/glass-remote.desktop, glass-remote-settings.desktop"

if ! ldconfig -p 2>/dev/null | grep -q 'libSDL2-2.0.so.0' && [ ! -e /usr/lib/libSDL2-2.0.so.0 ]; then
  echo "note: SDL2 was not found; the display needs it:  sudo apt install libsdl2-2.0-0"
fi
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) echo "note: $PREFIX/bin is not on PATH; the desktop entries use the full path" ;;
esac

if [ "$SERVICE" = yes ]; then
  UNIT_DIR=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
  mkdir -p "$UNIT_DIR"
  sed "s|@BIN@|$PREFIX/bin/glass|g" "$HERE/glass-remote.service" > "$UNIT_DIR/glass-remote.service"
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    systemctl --user enable --now glass-remote.service
    echo "service: glass-remote.service enabled for this user (journalctl --user -u glass-remote)"
    echo "note: for a display that must run before anyone logs in:  sudo loginctl enable-linger $USER"
  else
    echo "service: unit written to $UNIT_DIR/glass-remote.service (systemctl not found)"
  fi
fi

echo "run:  glass --remote            (the display; its settings page is on port 5583)"
echo "      glass --remote --settings (the settings page in a browser)"
