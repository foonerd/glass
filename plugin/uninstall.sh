#!/bin/bash
# Glass uninstaller. Themes stay when the settings asked for it; the
# settings backups stay always.

# What Glass keeps under Internal Storage, cleared: the settings backups
# always (they are what a player is put back from after an uninstall, and
# a backup removed with the plugin it backs up would be none), and where
# the user asked to keep the themes, the themes: the meter themes, the
# spectrum themes, the face themes, the uploaded fonts, and the ask itself.
# Everything else goes either way: glass-evo and the decoder (fetched
# again at a press), the catalogue's cache, the previews, the kept zips
# and the Manager's own state: a reinstall carries nothing but the themes
# and the backups, and takes its settings from the newest backup.
clear_data() {
  local dir=$1
  [ -d "$dir" ] || return 0
  # The engine's configuration goes with the plugin's config.json, which
  # Volumio removes at the same moment: a settings pair does not survive in halves.
  rm -rf "$dir/config"
  if [ -f "$dir/.preserve" ]; then
    echo "Keeping the themes, the fonts and the backups under $dir (user setting); the rest goes"
    find "$dir" -mindepth 1 -maxdepth 1 ! -name backups ! -name templates ! -name templates_spectrum ! -name faces ! -name fonts ! -name .preserve -exec rm -rf {} +
    return 0
  fi
  echo "Removing the themes under $dir; the settings backups stay"
  find "$dir" -mindepth 1 -maxdepth 1 ! -name backups -exec rm -rf {} +
  # A backups folder with nothing in it goes, and the folder itself once it is empty.
  rmdir "$dir/backups" 2>/dev/null || true
  rmdir "$dir" 2>/dev/null || true
}

# The screen back to the kiosk before anything goes. A take by glass-evo
# recorded every change in the register; the plugin's own way back cannot
# run once this script has cleared the data folder (Volumio runs it within
# seconds of the stop), so the units are restored here, as root, in
# reverse order, the face's X server first. The kiosk plugins the take
# turned off are the player's to turn on: the register, marked, goes into
# the backups folder as screen-owner.last.json, which the next install
# reads. SYSTEMCTL, X0_SOCKET and X0_WAIT_S are the tests' to override;
# node is the player's own.
give_screen_back() {
  local dir=$1
  local register="$dir/screen-owner.json"
  [ -f "$register" ] || return 0
  local sc=${SYSTEMCTL:-systemctl}
  local x0=${X0_SOCKET:-/tmp/.X11-unix/X0}
  local wait_s=${X0_WAIT_S:-10}
  local steps
  steps=$(node -e '
    const fs = require("fs");
    let r = null;
    try { r = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); } catch (e) { process.exit(0); }
    if (!r || r.owner !== "glass-evo" || r.gaveBackAt) process.exit(0);
    const changes = Array.isArray(r.changes) ? r.changes.slice().reverse() : [];
    for (const c of changes) {
      if (c.kind === "own-x") console.log("own-x " + c.name);
      else if (c.kind === "unit" && c.was && (c.was.enabled || c.was.active)) console.log("unit " + c.name + " " + (c.was.enabled ? 1 : 0) + " " + (c.was.active ? 1 : 0));
      else if (c.kind === "plugin" && c.was && c.was.enabled) console.log("plugin " + c.name);
    }
  ' "$register" 2>/dev/null) || steps=""
  local plugins=""
  if [ -n "$steps" ]; then
    echo "Giving the screen back to the kiosk"
    local kind name enable start i
    while read -r kind name enable start; do
      case "$kind" in
        own-x)
          echo "  $name stopped and disabled"
          $sc stop "$name" 2>/dev/null || true
          $sc disable "$name" 2>/dev/null || true
          $sc reset-failed "$name" 2>/dev/null || true
          i=0
          while [ -e "$x0" ] && [ "$i" -lt "$((wait_s * 10))" ]; do sleep 0.1; i=$((i + 1)); done
          ;;
        unit)
          echo "  $name$([ "$enable" = 1 ] && echo ' enabled')$([ "$start" = 1 ] && echo ' started')"
          if [ "$enable" = 1 ]; then $sc enable "$name" 2>/dev/null || true; fi
          if [ "$start" = 1 ]; then $sc start "$name" 2>/dev/null || true; fi
          ;;
        plugin) plugins="$plugins $name" ;;
      esac
    done <<STEPS
$steps
STEPS
    [ -n "$plugins" ] && echo "  the kiosk plugins the take turned off, for the next install to turn on:$plugins"
    # A kiosk enabled at boot but not running when the take came stays as
    # it was by the register's word; an uninstall leaves a usable screen,
    # so an enabled kiosk is started where no X server stands.
    if $sc is-enabled volumio-kiosk >/dev/null 2>&1 && ! $sc is-active volumio-kiosk >/dev/null 2>&1 && [ ! -e "$x0" ]; then
      echo "  volumio-kiosk started (enabled at boot, not running when the take came)"
      $sc start volumio-kiosk 2>/dev/null || true
    fi
  fi
  mkdir -p "$dir/backups"
  node -e '
    const fs = require("fs");
    try {
      const r = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      if (!r.gaveBackAt) { r.gaveBackAt = new Date().toISOString(); r.gaveBackBy = "uninstall"; }
      r.pluginsPending = String(process.argv[3] || "").split(" ").filter(Boolean);
      fs.writeFileSync(process.argv[2], JSON.stringify(r, null, 2));
    } catch (e) {}
  ' "$register" "$dir/backups/screen-owner.last.json" "$plugins" 2>/dev/null || true
  rm -f "$register"
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

rm -f "$PLUGIN_DIR/lib/libglasstap.so" "$PLUGIN_DIR/lib/libevotap.so" /dev/shm/glasstap.* /dev/shm/evotap.* /dev/shm/evometer.* /dev/shm/evoaudio.*
rm -rf "$PLUGIN_DIR/fanart-cache"

give_screen_back "$DATA_DIR"
clear_data "$DATA_DIR"

echo "Glass uninstalled"
echo "pluginuninstallend"
