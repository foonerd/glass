#!/bin/bash
# Glass uninstaller. Themes stay when the settings asked for it; the
# settings backups stay always.

# What Glass keeps under Internal Storage, cleared: everything, where the
# user did not ask to keep the themes, but the settings backups. They are
# what a player is put back from after an uninstall, and a backup removed
# with the plugin it backs up would be none.
clear_data() {
  local dir=$1
  [ -d "$dir" ] || return 0
  if [ -f "$dir/.preserve" ]; then
    echo "Keeping the themes and backups under $dir (user setting)"
    return 0
  fi
  echo "Removing the themes under $dir; the settings backups stay"
  find "$dir" -mindepth 1 -maxdepth 1 ! -name backups -exec rm -rf {} +
  # A backups folder with nothing in it goes, and the folder itself once it is empty.
  rmdir "$dir/backups" 2>/dev/null || true
  rmdir "$dir" 2>/dev/null || true
}

# Sourced for its functions alone, by the tests.
[ -n "${GLASS_UNINSTALL_FUNCTIONS:-}" ] && return 0

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

clear_data "$DATA_DIR"

echo "Glass uninstalled"
echo "pluginuninstallend"
