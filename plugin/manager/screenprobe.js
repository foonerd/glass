'use strict';
// What the player has for a screen, read from the kernel and the system
// and said in plain words: the panels and their native modes, the touch
// panels, mice and keyboards, the backlights, and who holds the screen
// now (the kiosk, the plugins that run it, an X server). The parsers take
// texts and are tested on texts captured from a player; the reader that
// gathers the texts is the only part that touches the machine.

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

// /proc/bus/input/devices: one block per device, its lines keyed by a
// letter. The name, the handlers (the event node among them), the
// physical path and the sysfs path are what the probe uses.
function parseInputDevices(text) {
  const devices = [];
  String(text || '').split(/\n\s*\n/).forEach(function (block) {
    const d = {};
    block.split('\n').forEach(function (line) {
      const m = /^([A-Z]): (.*)$/.exec(line.trim());
      if (!m) return;
      const body = m[2];
      if (m[1] === 'N') d.name = (/Name="(.*)"/.exec(body) || [null, ''])[1];
      else if (m[1] === 'P') d.phys = (/Phys=(.*)/.exec(body) || [null, ''])[1];
      else if (m[1] === 'S') d.sysfs = (/Sysfs=(.*)/.exec(body) || [null, ''])[1];
      else if (m[1] === 'H') {
        d.handlers = (/Handlers=(.*)/.exec(body) || [null, ''])[1].trim().split(/\s+/).filter(Boolean);
        const ev = d.handlers.find(h => /^event\d+$/.test(h));
        if (ev) d.event = '/dev/input/' + ev;
      }
    });
    if (d.name !== undefined) devices.push(d);
  });
  return devices;
}

// The properties udev gives each event node, one block per node headed
// by `## /dev/input/eventN`: the ID_INPUT_* classes are the ones read.
function parseUdev(text) {
  const nodes = {};
  let current = null;
  String(text || '').split('\n').forEach(function (line) {
    const head = /^## (\/dev\/input\/event\d+)/.exec(line.trim());
    if (head) { current = head[1]; nodes[current] = {}; return; }
    const kv = /^([A-Z_]+)=(.*)$/.exec(line.trim());
    if (kv && current) nodes[current][kv[1]] = kv[2];
  });
  return nodes;
}

// Each input device by what udev says it is. A touch panel, a mouse and a
// keyboard are what the screen cares about; a pointing stick from an HDMI
// remote (CEC) is not a mouse, though SDL would take it for one, so it is
// named as a remote. Everything else is passed over.
function classifyInputs(devices, udev) {
  const out = { touch: [], mice: [], keyboards: [], remotes: [] };
  devices.forEach(function (d) {
    const p = (d.event && udev[d.event]) || {};
    const item = { name: d.name, event: d.event || null };
    if (p.ID_INPUT_TOUCHSCREEN === '1' || p.ID_INPUT_TABLET === '1') out.touch.push(item);
    else if (p.ID_INPUT_MOUSE === '1' || p.ID_INPUT_TOUCHPAD === '1') out.mice.push(item);
    else if (p.ID_INPUT_KEYBOARD === '1') out.keyboards.push(item);
    else if (p.ID_INPUT_POINTINGSTICK === '1' && /hdmi|cec/i.test(d.name + ' ' + (d.phys || ''))) out.remotes.push(item);
  });
  return out;
}

// The DRM connectors, one block per connector headed by `## cardN-NAME`:
// status, whether enabled, the modes in the kernel's order (the first is
// the preferred, native mode), and whether an EDID was read. A connector
// whose native mode is taller than wide is a portrait panel.
function parseDrm(text) {
  const connectors = [];
  let current = null;
  String(text || '').split('\n').forEach(function (line) {
    const head = /^## (card\d+)-(.+)$/.exec(line.trim());
    if (head) { current = { card: head[1], name: head[2], status: '', enabled: false, modes: [], edid: false }; connectors.push(current); return; }
    const kv = /^([a-z_]+)=(.*)$/.exec(line.trim());
    if (!kv || !current) return;
    if (kv[1] === 'status') current.status = kv[2];
    else if (kv[1] === 'enabled') current.enabled = kv[2] === 'enabled';
    else if (kv[1] === 'modes') current.modes = kv[2].trim().split(/\s+/).filter(Boolean);
    else if (kv[1] === 'edid_bytes') current.edid = parseInt(kv[2], 10) > 0;
  });
  connectors.forEach(function (c) {
    const m = /^(\d+)x(\d+)/.exec(c.modes[0] || '');
    c.native = m ? { width: parseInt(m[1], 10), height: parseInt(m[2], 10) } : null;
    c.portrait = !!(c.native && c.native.height > c.native.width);
    c.kind = /^HDMI/.test(c.name) ? 'hdmi' : /^DSI/.test(c.name) ? 'dsi' : /^DPI/.test(c.name) ? 'dpi' : /^Writeback/.test(c.name) ? 'writeback' : 'other';
  });
  return connectors.filter(c => c.kind !== 'writeback');
}

// The backlights under /sys/class/backlight, one block each: the current
// and the greatest brightness.
function parseBacklights(text) {
  const lights = [];
  let current = null;
  String(text || '').split('\n').forEach(function (line) {
    const head = /^## (.+)$/.exec(line.trim());
    if (head) { current = { name: head[1], brightness: null, max: null, power: null }; lights.push(current); return; }
    const kv = /^([a-z_]+)=(.*)$/.exec(line.trim());
    if (!kv || !current) return;
    if (kv[1] === 'brightness') current.brightness = parseInt(kv[2], 10);
    else if (kv[1] === 'max_brightness') current.max = parseInt(kv[2], 10);
    else if (kv[1] === 'bl_power') current.power = parseInt(kv[2], 10);
  });
  return lights.filter(l => l.max !== null && !isNaN(l.max));
}

// The suggestion the probe makes, from what it found and what the Touch
// Display plugin has on file: the rotation for a portrait panel (the
// plugin's angle when it has one, else 270, the way the common Pi panels
// are mounted), and whether the pointer shows (a mouse shows it, a touch
// panel alone hides it).
function suggest(found) {
  const panel = (found.connectors || []).find(c => c.status === 'connected') || null;
  const rotation = panel && panel.portrait
    ? ([90, 180, 270].indexOf(found.touchDisplayAngle) !== -1 ? found.touchDisplayAngle : 270)
    : 0;
  const pointer = (found.inputs && found.inputs.mice.length) ? 'show' : 'hide';
  return { rotation: rotation, pointer: pointer, panel: panel ? panel.name : null };
}

// The pointer as the display reads it, from the choice: `auto` follows the
// suggestion, `show` and `hide` say so.
function pointerShown(choice, found) {
  if (choice === 'show') return true;
  if (choice === 'hide') return false;
  return suggest(found).pointer === 'show';
}

// Everything, read from the machine. Every reader is guarded: a file or a
// command that is not there leaves its part empty rather than failing the
// probe. `deps` lets a test hand in the texts.
function gather(deps) {
  const d = Object.assign({
    readFile(p) { return fs.readFileSync(p, 'utf8'); },
    readDir(p) { return fs.readdirSync(p); },
    exists(p) { return fs.existsSync(p); },
    exec(cmd, args) { return execFileSync(cmd, args, { encoding: 'utf8', timeout: 3000, stdio: ['ignore', 'pipe', 'ignore'] }); }
  }, deps || {});
  const safe = function (fn, fallback) { try { return fn(); } catch (e) { return fallback; } };
  const inputsText = safe(() => d.readFile('/proc/bus/input/devices'), '');
  const devices = parseInputDevices(inputsText);
  const udevText = devices.filter(x => x.event).map(function (x) {
    return '## ' + x.event + '\n' + safe(() => d.exec('udevadm', ['info', '--query=property', '--name=' + x.event]), '');
  }).join('\n');
  const udev = parseUdev(udevText);
  const drmText = safe(() => d.readDir('/sys/class/drm'), []).filter(n => /^card\d+-/.test(n)).map(function (n) {
    const base = '/sys/class/drm/' + n;
    const read = f => safe(() => d.readFile(path.join(base, f)).trim(), '');
    const edid = safe(() => d.readFile(path.join(base, 'edid')).length, 0);
    return '## ' + n + '\nstatus=' + read('status') + '\nenabled=' + read('enabled') + '\nmodes=' + read('modes').split('\n').join(' ') + '\nedid_bytes=' + edid;
  }).join('\n');
  const backlightText = safe(() => d.readDir('/sys/class/backlight'), []).map(function (n) {
    const base = '/sys/class/backlight/' + n;
    const read = f => safe(() => d.readFile(path.join(base, f)).trim(), '');
    return '## ' + n + '\nbrightness=' + read('brightness') + '\nmax_brightness=' + read('max_brightness') + '\nbl_power=' + read('bl_power');
  }).join('\n');
  const plugins = safe(() => JSON.parse(d.readFile('/data/configuration/plugins.json')), {});
  const enabled = function (category, name) {
    const p = plugins && plugins[category] && plugins[category][name];
    return !!(p && p.enabled && p.enabled.value === true);
  };
  const touchDisplay = safe(() => JSON.parse(d.readFile('/data/configuration/user_interface/touch_display/config.json')), null);
  const angle = touchDisplay && touchDisplay.angle ? parseInt(touchDisplay.angle.value, 10) : NaN;
  const kiosk = safe(() => d.exec('systemctl', ['is-active', 'volumio-kiosk']).trim() === 'active', false);
  const found = {
    board: safe(() => d.readFile('/proc/device-tree/model').replace(/\0/g, '').trim(), ''),
    connectors: parseDrm(drmText),
    inputs: classifyInputs(devices, udev),
    backlights: parseBacklights(backlightText),
    holders: {
      kiosk: kiosk,
      xserver: d.exists('/tmp/.X11-unix/X0'),
      touchDisplay: enabled('user_interface', 'touch_display'),
      displayConfiguration: enabled('user_interface', 'display_configuration')
    },
    touchDisplayAngle: isNaN(angle) ? null : angle
  };
  found.suggestion = suggest(found);
  return found;
}

// The fact of the screen: whether it is free for the display to draw on
// itself. Free means no X server up, the kiosk unit neither running nor
// starting nor enabled at boot, neither the Touch Display nor the Display
// Configuration plugin on (either brings the kiosk), and a panel connected
// to draw on. Anything else, the kiosk is or will be the player's
// interface on the screen, and the display leaves it be.
function screenFree(fact) {
  const f = fact || {};
  const kiosk = String(f.kiosk || 'inactive');
  return !f.xserver && kiosk !== 'active' && kiosk !== 'activating' && kiosk !== 'reloading'
    && !f.kioskEnabled && !f.touchDisplay && !f.displayConfiguration && !!f.panel;
}

// What the display draws on: X while an X server is up, the screen itself
// while the screen is free, else nothing (no window: the kiosk is on its
// way, or there is no panel).
function wouldDraw(fact) {
  const f = fact || {};
  if (f.xserver) return 'x11';
  return screenFree(f) ? 'kmsdrm' : null;
}

module.exports = { parseInputDevices, parseUdev, classifyInputs, parseDrm, parseBacklights, suggest, pointerShown, screenFree, wouldDraw, gather };
