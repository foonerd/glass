#!/bin/bash
# Display setup for the glass binary. Same rules as the Peppy launcher:
# X11 when a socket exists, x64 renderer and MIT-SHM fixes, software fallback
# when the render node is present but not usable. Audio paths stay outside.

log() {
  echo "glass-launcher: $*"
}

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

if [ -n "$GLASS_BIN" ]; then
  BIN=$GLASS_BIN
else
  ARCH_DIR=$(sed -n 's/^VOLUMIO_ARCH="\(.*\)"/\1/p' /etc/os-release 2>/dev/null | head -n1)
  if [ -z "$ARCH_DIR" ]; then
    case $(uname -m) in
      x86_64) ARCH_DIR=x64 ;;
      aarch64) ARCH_DIR=armv8 ;;
      armv7l|armv6l) ARCH_DIR=armv7 ;;
      *) ARCH_DIR=$(uname -m) ;;
    esac
  fi
  if [ -x "$ROOT/bin/$ARCH_DIR/glass" ]; then
    BIN=$ROOT/bin/$ARCH_DIR/glass
  else
    BIN=$(command -v glass || true)
  fi
fi

if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "glass-launcher: glass binary not found" >&2
  exit 1
fi

if ! "$BIN" --help >/dev/null 2>&1; then
  echo "glass-launcher: the shipped binary for this machine is missing or is the wrong CPU." >&2
  echo "glass-launcher: expected $ROOT/bin/$ARCH_DIR/glass" >&2
  exit 126
fi

ARCH=$(sed -n 's/^VOLUMIO_ARCH="\(.*\)"/\1/p' /etc/os-release 2>/dev/null | head -n1)
if [ -z "$ARCH" ]; then
  case $(uname -m) in
    x86_64) ARCH=x64 ;;
    aarch64) ARCH=armv8 ;;
    armv7l) ARCH=armv7 ;;
    *) ARCH=$(uname -m) ;;
  esac
  log "VOLUMIO_ARCH missing, using uname ($ARCH)"
fi

export DISPLAY=${DISPLAY:-:0}

if [ -z "$XAUTHORITY" ] || [ ! -f "$XAUTHORITY" ]; then
  X_PID=$(pgrep -xo Xorg 2>/dev/null || true)
  if [ -z "$X_PID" ]; then
    X_PID=$(pgrep -xo X 2>/dev/null || true)
  fi
  if [ -n "$X_PID" ] && [ -r "/proc/$X_PID/cmdline" ]; then
    X_CMDLINE=$(tr '\0' ' ' < "/proc/$X_PID/cmdline")
    AUTH_FROM_X=$(echo "$X_CMDLINE" | sed -n 's/.*-auth[[:space:]]\([^[:space:]]\+\).*/\1/p')
    if [ -n "$AUTH_FROM_X" ] && [ -f "$AUTH_FROM_X" ]; then
      export XAUTHORITY=$AUTH_FROM_X
    fi
  fi
fi

if [ -z "$XAUTHORITY" ] || [ ! -f "$XAUTHORITY" ]; then
  LATEST_AUTH=$(ls -1t /tmp/serverauth.* 2>/dev/null | head -n1)
  if [ -n "$LATEST_AUTH" ] && [ -f "$LATEST_AUTH" ]; then
    export XAUTHORITY=$LATEST_AUTH
  fi
fi

DISPLAY_NUM=${DISPLAY#:}
if [ -S "/tmp/.X11-unix/X${DISPLAY_NUM}" ]; then
  export SDL_VIDEODRIVER=x11
  if [ -n "$XAUTHORITY" ] && [ -f "$XAUTHORITY" ]; then
    log "backend=x11 display=$DISPLAY xauth=$XAUTHORITY"
  else
    log "backend=x11 display=$DISPLAY xauth=missing"
  fi
elif [ -n "$WAYLAND_DISPLAY" ]; then
  export SDL_VIDEODRIVER=wayland
  log "backend=wayland display=$WAYLAND_DISPLAY"
else
  log "backend=unknown"
fi

if [ "$ARCH" = "x64" ]; then
  export LIBGL_ALWAYS_SOFTWARE=0
  export SDL_RENDER_DRIVER=x11
  export SDL_FRAMEBUFFER_ACCELERATION=0
  export QT_X11_NO_MITSHM=1
  export _X11_NO_MITSHM=1
  log "arch=x64 render=x11 mit-shm=off"
fi

if [ -e /dev/dri/renderD128 ] && { [ ! -r /dev/dri/renderD128 ] || [ ! -w /dev/dri/renderD128 ]; }; then
  export LIBGL_ALWAYS_SOFTWARE=1
  export SDL_RENDER_DRIVER=software
  export MESA_LOADER_DRIVER_OVERRIDE=llvmpipe
  log "render-node inaccessible; software rendering"
fi

exec "$BIN" "$@"
