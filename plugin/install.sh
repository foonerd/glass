#!/bin/bash
# Glass installer. Runs as root through the plugin manager; its output is
# shown in the install dialog and a non-zero exit fails the install.
echo "Installing Glass"

ARCH=$(cat /etc/os-release | grep ^VOLUMIO_ARCH | tr -d 'VOLUMIO_ARCH="')
VARIANT=$(cat /etc/os-release | grep ^VOLUMIO_VARIANT | tr -d 'VOLUMIO_VARIANT="')
HARDWARE=$(cat /etc/os-release | grep ^VOLUMIO_HARDWARE | tr -d 'VOLUMIO_HARDWARE="')

PLUGIN_DIR="/data/plugins/user_interface/glass"
DATA_DIR="/data/INTERNAL/glass"
LEGACY_DIR="/data/plugins/user_interface/peppy_screensaver"
LEGACY_DATA="/data/INTERNAL/peppy_screensaver"
RENDER_MARKER="/etc/glass_render_group_added"

# A refused install leaves nothing behind, so the next attempt starts clean.
refuse() {
  echo ""
  echo "$@"
  rm -rf "$PLUGIN_DIR"
  exit 1
}

if [ -z "$ARCH" ]; then
  refuse "ERROR: Could not detect the Volumio architecture"
fi
echo "Architecture: $ARCH, variant: ${VARIANT:-unknown}, hardware: ${HARDWARE:-unknown}"

# =============================================================================
# GUARD: Glass replaces PeppyMeter Screensaver; the two cannot share the audio path
# =============================================================================
if [ -d "$LEGACY_DIR" ] && [ -f /data/configuration/plugins.json ]; then
  LEGACY_ENABLED=$(node -e 'try { var d = require("/data/configuration/plugins.json"); var p = d.user_interface && d.user_interface.peppy_screensaver; console.log(p && p.enabled && p.enabled.value === true ? "yes" : "no"); } catch (e) { console.log("no"); }' 2>/dev/null)
  if [ "$LEGACY_ENABLED" = "yes" ]; then
    refuse "ERROR: PeppyMeter Screensaver is enabled.
Glass replaces PeppyMeter Screensaver and cannot be installed while it is enabled.
Disable PeppyMeter Screensaver under Plugins, then install Glass again.
Your themes and settings are taken over."
  fi
fi

# =============================================================================
# CHECK: the display binary for this machine. The unzip does not keep modes.
# =============================================================================
chmod +x "$PLUGIN_DIR"/bin/*/glass "$PLUGIN_DIR"/bin/*/tapdump "$PLUGIN_DIR"/bin/*/glass-serve "$PLUGIN_DIR/run_glass.sh" 2>/dev/null || true
if [ ! -x "$PLUGIN_DIR/bin/$ARCH/glass" ]; then
  refuse "ERROR: no display binary for architecture $ARCH (available: $(ls "$PLUGIN_DIR/bin" 2>/dev/null | tr '\n' ' '))"
fi
chmod +x "$PLUGIN_DIR/run_glass.sh"

# =============================================================================
# INSTALL: runtime dependencies
# =============================================================================
NEEDED_PKGS=""
for pkg in libsdl2-2.0-0; do
  if ! dpkg -s "$pkg" > /dev/null 2>&1; then
    NEEDED_PKGS="$NEEDED_PKGS $pkg"
  fi
done
if [ -n "$NEEDED_PKGS" ]; then
  echo "Installing:$NEEDED_PKGS"
  apt-get update
  apt-get install -y $NEEDED_PKGS
fi

# =============================================================================
# INSTALL: the tap, the ALSA scope that measures what plays
# =============================================================================
if [ ! -f "$PLUGIN_DIR/lib/$ARCH/libglasstap.so" ]; then
  refuse "ERROR: no tap library for architecture $ARCH"
fi
ln -sf "$PLUGIN_DIR/lib/$ARCH/libglasstap.so" "$PLUGIN_DIR/lib/libglasstap.so"
echo "Tap: $PLUGIN_DIR/lib/$ARCH/libglasstap.so"

# =============================================================================
# SETUP: configuration files, kept across upgrades
# =============================================================================
mkdir -p "$PLUGIN_DIR/config" "$PLUGIN_DIR/asound"
for name in meter spectrum; do
  if [ ! -f "$PLUGIN_DIR/config/$name.txt" ]; then
    cp "$PLUGIN_DIR/config/$name.txt.tmpl" "$PLUGIN_DIR/config/$name.txt"
  fi
done

# =============================================================================
# SETUP: themes. Adopt what PeppyMeter Screensaver left, then add the bundled ones.
# =============================================================================
mkdir -p "$DATA_DIR"
if [ -d "$LEGACY_DATA/templates" ]; then
  if [ ! -d "$LEGACY_DIR" ] && [ ! -d "$DATA_DIR/templates" ]; then
    # PeppyMeter Screensaver was uninstalled with its themes preserved: the directory is ours.
    echo "Adopting the preserved PeppyMeter Screensaver themes in place..."
    rm -rf "$DATA_DIR"
    mv "$LEGACY_DATA" "$DATA_DIR"
  else
    # Still installed (disabled), or Glass already has themes: copy what is missing.
    echo "Copying PeppyMeter Screensaver themes..."
    FREE_KB=$(df -Pk "$DATA_DIR" | awk 'NR==2 {print $4}')
    NEED_KB=$(du -sk "$LEGACY_DATA" | awk '{print $1}')
    if [ "$FREE_KB" -gt $((NEED_KB + 51200)) ]; then
      mkdir -p "$DATA_DIR/templates" "$DATA_DIR/templates_spectrum" "$DATA_DIR/backups"
      cp -rn "$LEGACY_DATA/templates/." "$DATA_DIR/templates/" 2>/dev/null || true
      [ -d "$LEGACY_DATA/templates_spectrum" ] && cp -rn "$LEGACY_DATA/templates_spectrum/." "$DATA_DIR/templates_spectrum/" 2>/dev/null || true
      [ -d "$LEGACY_DATA/backups" ] && cp -rn "$LEGACY_DATA/backups/." "$DATA_DIR/backups/" 2>/dev/null || true
      [ -f "$LEGACY_DATA/.preserve" ] && touch "$DATA_DIR/.preserve"
    else
      echo "Not enough free space to copy the themes ($NEED_KB kB needed, $FREE_KB kB free); install them from the catalog later."
    fi
  fi
fi
mkdir -p "$DATA_DIR/templates" "$DATA_DIR/templates_spectrum"
if [ -d "$PLUGIN_DIR/templates" ]; then
  cp -rn "$PLUGIN_DIR/templates/." "$DATA_DIR/templates/"
  rm -rf "$PLUGIN_DIR/templates"
fi
if [ -d "$PLUGIN_DIR/templates_spectrum" ]; then
  cp -rn "$PLUGIN_DIR/templates_spectrum/." "$DATA_DIR/templates_spectrum/"
  rm -rf "$PLUGIN_DIR/templates_spectrum"
fi
chmod -R 755 "$DATA_DIR"
chown -R volumio:volumio "$DATA_DIR"
chown -R volumio:volumio "$PLUGIN_DIR"

# =============================================================================
# SETUP: X11 access on kiosk products and x64 images
# =============================================================================
KIOSK_DEVICE="false"
if [ -f /opt/volumiokiosk.sh ] || [ "$VARIANT" = "motivo" ] || [ "$VARIANT" = "primo" ]; then
  KIOSK_DEVICE="true"
fi
if [ "$KIOSK_DEVICE" = "true" ] || [ "$ARCH" = "x64" ]; then
  cat > /etc/X11/Xsession.d/50-glass-xhost << 'XHOSTEOF'
#!/bin/sh
# Let the volumio user open a window on the local X session for Glass.
xhost +SI:localuser:volumio >/dev/null 2>&1 || xhost +local: >/dev/null 2>&1
XHOSTEOF
  chmod 644 /etc/X11/Xsession.d/50-glass-xhost
  if getent group render >/dev/null 2>&1; then
    if ! id -nG volumio | tr ' ' '\n' | grep -qx render; then
      usermod -aG render volumio
      touch "$RENDER_MARKER"
      echo "Added volumio to the render group (reboot required)"
    fi
  fi
fi

echo "Glass installed"
echo "plugininstallend"
