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
  return out;
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
function state(last, wish, display) {
  var r = last || {};
  return {
    power: r.power || (wish && wish.power === 'off' ? 'off' : 'on'),
    by: r.by || '',
    brightness: r.brightness === undefined ? null : r.brightness,
    maxBrightness: r.maxBrightness === undefined ? null : r.maxBrightness,
    backlight: !!(r.maxBrightness),
    ours: r.ours === undefined ? !!display.ours : r.ours,
    display: !!display.running,
    reported: !!last
  };
}

module.exports = { request, report, state };
