'use strict';
// The panel's power as the plugin holds it: what the player, the Manager or
// a remote asks (on, off, toggle, a brightness), what the display reports
// back (the power, by what means, the backlight's brightness and its top,
// whether the screen is the display's own), and the state the two make.

// A request as it came in: `power` on, off or toggle, `brightness` a whole
// number; anything else refused. An empty request asks for the state.
function request(data) {
  var d = data && typeof data === 'object' ? data : {};
  var out = {};
  if (d.power !== undefined && d.power !== null && d.power !== '') {
    var power = String(d.power).trim().toLowerCase();
    if (power === 'true' || power === '1') power = 'on';
    if (power === 'false' || power === '0') power = 'off';
    if (['on', 'off', 'toggle'].indexOf(power) === -1) return { error: 'bad-power' };
    out.power = power;
  }
  if (d.brightness !== undefined && d.brightness !== null && d.brightness !== '') {
    var level = Number(d.brightness);
    if (!isFinite(level) || level < 0 || Math.floor(level) !== level) return { error: 'bad-brightness' };
    out.brightness = level;
  }
  // Dim while nothing plays: the switch, and the brightness when idle.
  if (d.dim !== undefined && d.dim !== null && d.dim !== '') {
    var dim = String(d.dim).trim().toLowerCase();
    if (['on', 'true', '1', 'yes'].indexOf(dim) !== -1) out.dim = true;
    else if (['off', 'false', '0', 'no'].indexOf(dim) !== -1) out.dim = false;
    else return { error: 'bad-dim' };
  }
  if (d.idleBrightness !== undefined && d.idleBrightness !== null && d.idleBrightness !== '') {
    var idle = Number(d.idleBrightness);
    if (!isFinite(idle) || idle < 0 || Math.floor(idle) !== idle) return { error: 'bad-idle-brightness' };
    out.idleBrightness = idle;
  }
  return out;
}

// The dim settings as the plugin keeps them: whether the panel dims while
// nothing plays (off unless turned on), the brightness then, and the
// brightness at play; a level not set is null.
function settings(config) {
  var get = function (key) { try { return config.get(key); } catch (e) { return undefined; } };
  var level = function (key) { var n = parseInt(get(key), 10); return isNaN(n) || n < 0 ? null : n; };
  return { dim: String(get('screenDimIdle')) === 'true', idleLevel: level('screenBrightnessIdle'), level: level('screenBrightness') };
}

// The brightness the display is to hold: the idle level while the panel
// is dimmed and the switch is on and a level is set, else the brightness
// at play; null when nothing is known.
function wantedLevel(s, dimmed) {
  if (dimmed && s.dim && s.idleLevel !== null) return s.idleLevel;
  return s.level;
}

// How long after a genuine stop the panel dims: at the moment the clock
// comes, which is at once, or after the persist period when the look
// waits for it.
function dimDelayMs(idleWait, persistS) {
  return String(idleWait || 'none') === 'persist' ? Math.max(0, Number(persistS) || 0) * 1000 : 0;
}

// The display's report, kept as it came: the power, the means, the
// brightness and its top where there is a backlight, and whose the screen is.
function report(message) {
  var m = message && typeof message === 'object' ? message : {};
  var by = String(m.by || '');
  if (['backlight', 'dpms', 'dark', ''].indexOf(by) === -1) by = '';
  var brightness = Number(m.brightness);
  var max = Number(m.max);
  var lit = isFinite(brightness) && isFinite(max) && max > 0;
  return {
    power: String(m.power) === 'off' ? 'off' : 'on',
    by: by,
    brightness: lit ? brightness : null,
    maxBrightness: lit ? max : null,
    ours: m.ours === true,
    at: Date.now()
  };
}

// The state as answered: the display's last report where there is one,
// else what the plugin asked for, and whether a display is there to ask.
function state(last, wish, display, idle) {
  var r = last || {};
  var i = idle || {};
  return {
    power: r.power || (wish && wish.power === 'off' ? 'off' : 'on'),
    by: r.by || '',
    brightness: r.brightness === undefined ? null : r.brightness,
    maxBrightness: r.maxBrightness === undefined ? null : r.maxBrightness,
    backlight: !!(r.maxBrightness),
    ours: r.ours === undefined ? !!display.ours : r.ours,
    display: !!display.running,
    reported: !!last,
    // Dim while nothing plays: the switch, the brightness when idle, the
    // brightness at play, and whether the panel is dimmed now.
    idle: { on: !!i.dim, level: i.idleLevel === undefined ? null : i.idleLevel, bright: i.level === undefined ? null : i.level, dimmed: !!i.dimmed }
  };
}

module.exports = { request, report, state, settings, wantedLevel, dimDelayMs };
