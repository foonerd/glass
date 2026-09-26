#!/bin/sh
# Remove what install.sh put in place for this user. The configuration
# (~/.config/glass-remote) and the cache (~/.cache/glass-remote) stay unless
# --purge is given.
set -eu
PREFIX=${PREFIX:-$HOME/.local}
PURGE=no
for arg in "$@"; do
  case "$arg" in
    --purge) PURGE=yes ;;
    *) echo "uninstall.sh: unknown argument $arg" >&2; exit 2 ;;
  esac
done
UNIT_DIR=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
if [ -f "$UNIT_DIR/glass-remote.service" ]; then
  command -v systemctl >/dev/null 2>&1 && systemctl --user disable --now glass-remote.service 2>/dev/null || true
  rm -f "$UNIT_DIR/glass-remote.service"
  command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload || true
fi
rm -f "$PREFIX/bin/glass" \
      "$PREFIX/share/applications/glass-remote.desktop" \
      "$PREFIX/share/applications/glass-remote-settings.desktop" \
      "$PREFIX/share/icons/hicolor/scalable/apps/glass-remote.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
if [ "$PURGE" = yes ]; then
  rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/glass-remote" "${XDG_CACHE_HOME:-$HOME/.cache}/glass-remote"
  echo "removed the configuration and the cache"
fi
echo "removed"
