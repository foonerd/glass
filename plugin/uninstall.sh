#!/bin/bash
# Glass uninstaller. Themes stay when the settings asked for it.
echo "Uninstalling Glass"

PLUGIN_DIR="/data/plugins/user_interface/glass"
DATA_DIR="/data/INTERNAL/glass"
RENDER_MARKER="/etc/glass_render_group_added"

rm -f /tmp/glass_running /tmp/glass_dismiss /tmp/glass_persist
rm -f /data/configuration/music_service/mpd/mpd_custom.conf

if [ -f /etc/X11/Xsession.d/50-glass-xhost ]; then
  rm -f /etc/X11/Xsession.d/50-glass-xhost
fi
if [ -f "$RENDER_MARKER" ]; then
  gpasswd -d volumio render >/dev/null 2>&1 || true
  rm -f "$RENDER_MARKER"
fi

if [ -L "$PLUGIN_DIR/lib/libpeppyalsa.so" ]; then
  rm -f "$PLUGIN_DIR/lib/libpeppyalsa.so"
fi
rm -rf "$PLUGIN_DIR/fanart-cache"

if [ -d "$DATA_DIR" ]; then
  if [ -f "$DATA_DIR/.preserve" ]; then
    echo "Keeping the themes and backups under $DATA_DIR (user setting)"
  else
    echo "Removing the themes under $DATA_DIR"
    rm -rf "$DATA_DIR"
  fi
fi

echo "Glass uninstalled"
echo "pluginuninstallend"
