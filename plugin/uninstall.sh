#!/bin/bash
# Glass uninstaller. Themes stay when the settings asked for it.
echo "Uninstalling Glass"

PLUGIN_DIR="/data/plugins/user_interface/glass"
DATA_DIR="/data/INTERNAL/glass"
RENDER_MARKER="/etc/glass_render_group_added"

rm -f /tmp/glass_running /tmp/glass_dismiss /tmp/glass_persist /tmp/glass_channel

# What earlier releases put beside the tap: the MPD side output's include and
# the copies mounted over the player templates.
rm -f /data/configuration/music_service/mpd/mpd_custom.conf
umount /volumio/app/plugins/music_service/mpd/mpd.conf.tmpl 2>/dev/null || true
umount /volumio/app/plugins/music_service/airplay_emulation/shairport-sync.conf.tmpl 2>/dev/null || true
rm -f /tmp/mpd.conf.tmpl /tmp/shairport-sync.conf.tmpl

if [ -f /etc/X11/Xsession.d/50-glass-xhost ]; then
  rm -f /etc/X11/Xsession.d/50-glass-xhost
fi
if [ -f "$RENDER_MARKER" ]; then
  gpasswd -d volumio render >/dev/null 2>&1 || true
  rm -f "$RENDER_MARKER"
fi

rm -f "$PLUGIN_DIR/lib/libglasstap.so" /dev/shm/glasstap.*
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
