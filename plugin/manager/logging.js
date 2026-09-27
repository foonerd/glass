'use strict';

// What the plugin writes to the player's journal, gated by a level and,
// at the two finest levels, by targets. Every line the plugin writes is
// classified by its prefix into a target and a level; the gate compares.

const LEVELS = ['error', 'warn', 'info', 'verbose', 'trace'];
const TARGETS = ['audio', 'channel', 'display', 'remotes', 'themes', 'artwork', 'manager', 'settings'];
const DEFAULT_LEVEL = 'warn';

// Prefix rules, first match wins: the target the line belongs to, and the
// level an `info` call with that prefix really is.
const RULES = [
  [/^pushState/, 'channel', 'verbose'],
  [/^channel: command/, 'channel', 'verbose'],
  [/^channel:/, 'channel', 'info'],
  [/^Config version/, 'settings', 'verbose'],
  [/^glass-serve: .*\b(subscribed|gone)\b/, 'remotes', 'verbose'],
  [/^(glass-serve|remotes|beacon)\b/, 'remotes', 'info'],
  [/^(upgrade|manager)\b/, 'manager', 'info'],
  [/^(the display|dismissed|persist|grace|volatile|stop with|glass-launcher)/, 'display', 'info'],
  // The display's own lines, relayed as they come.
  [/^(frame\.rate=|meter=|leaving|snapshot |own theme|around again|synced from|remote .* of )/, 'display', 'info'],
  [/^(renderer|fonts loaded|memory |profile per|channel [^:]|channel gone|kept |fetched |brought )/, 'display', 'verbose'],
  [/^frames \d/, 'remotes', 'trace'],
  [/^(Soloist|spotify|Spotify|ALSA|the ALSA|the MPD side output)/, 'audio', 'info'],
  [/^(theme|applyActiveThemeFolder|removeTheme)/, 'themes', 'info'],
  [/fanart/i, 'artwork', 'info'],
  [/^(backup|settings|preserve flag|getUIConfig|listSettingsBackups)/i, 'settings', 'info'],
];

function classify(message) {
  const text = String(message || '');
  for (const [pattern, target, level] of RULES) {
    if (pattern.test(text)) return { target, level };
  }
  return { target: 'settings', level: 'info' };
}

function normalize(settings) {
  const s = settings || {};
  const level = LEVELS.includes(s.level) ? s.level : DEFAULT_LEVEL;
  const targets = Array.isArray(s.targets) ? s.targets.filter((t) => TARGETS.includes(t)) : [];
  return { level, targets };
}

// Whether a line at `level` about `target` passes the gate.
function allowed(settings, level, target) {
  const s = normalize(settings);
  const wanted = LEVELS.indexOf(s.level);
  const mine = LEVELS.indexOf(level);
  if (mine < 0 || mine > wanted) return false;
  if (mine >= LEVELS.indexOf('verbose') && s.targets.length && !s.targets.includes(target)) return false;
  return true;
}

// A logger in front of the player's: the same info, warn and error the
// plugin calls today, plus verbose and trace, each line classified and
// gated by the settings the function returns. `prefix` is what every
// line of the plugin starts with, dropped before classifying.
function makeLogger(base, prefix, settingsFn) {
  const strip = (message) => {
    const text = String(message === undefined ? '' : message);
    return text.startsWith(prefix) ? text.slice(prefix.length) : text;
  };
  const write = (call, message) => {
    try { base[call](message); } catch (e) { /* the player's logger is not ours to break */ }
  };
  const gated = (assumed, call) => (message) => {
    const found = classify(strip(message));
    const level = LEVELS.indexOf(found.level) > LEVELS.indexOf(assumed) ? found.level : assumed;
    if (allowed(settingsFn(), level, found.target)) write(call, message);
  };
  return {
    error: (message) => write('error', message),
    warn: (message) => { if (allowed(settingsFn(), 'warn', classify(strip(message)).target)) write('warn', message); },
    info: gated('info', 'info'),
    verbose: gated('verbose', 'info'),
    trace: gated('trace', 'info'),
  };
}

module.exports = { LEVELS, TARGETS, DEFAULT_LEVEL, classify, normalize, allowed, makeLogger };
