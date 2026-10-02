'use strict';
// What the status sheet says of the player beyond Glass itself: its
// memory, and the other plugins on it. Someone reading a report needs
// both: a board's name does not say how much memory it has, and another
// plugin in the audio path changes what the meters are fed. Pure: the
// plugin reads the files, these make the facts.

// The memory from /proc/meminfo's text, in megabytes: all of it, what is
// available to start something new, what is in use (the rest), and the
// swap in use; null when the text does not say.
function memoryOf(meminfo) {
  const kb = {};
  String(meminfo || '').split('\n').forEach(function (line) {
    const m = /^(\w+):\s+(\d+)\s*kB/.exec(line);
    if (m) kb[m[1]] = parseInt(m[2], 10);
  });
  if (!kb.MemTotal || kb.MemAvailable === undefined) return null;
  const mb = function (n) { return Math.round(n / 1024); };
  return {
    totalMb: mb(kb.MemTotal),
    availableMb: mb(kb.MemAvailable),
    usedMb: mb(kb.MemTotal - kb.MemAvailable),
    swapUsedMb: kb.SwapTotal ? mb(kb.SwapTotal - (kb.SwapFree || 0)) : 0
  };
}

// The other plugins installed on the player. `installed` is what the
// plugins folder holds, each a category, a name and its package.json's
// text; `registry` is the player's plugin registry (plugins.json, parsed).
// Each answers with its name as the player shows it, its version, whether
// it is on, and whether it puts itself in the audio path. Glass is left
// out; those in the audio path come first, then by name.
function pluginsOf(installed, registry) {
  const out = [];
  (installed || []).forEach(function (p) {
    if (p.category === 'user_interface' && p.name === 'glass') return;
    let pkg = {};
    try { pkg = JSON.parse(p.package) || {}; } catch (e) { pkg = {}; }
    const info = pkg.volumio_info || {};
    const state = ((registry || {})[p.category] || {})[p.name] || {};
    const value = function (v) { return v && typeof v === 'object' && 'value' in v ? v.value : v; };
    out.push({
      category: p.category,
      name: p.name,
      title: String(info.prettyName || pkg.name || p.name),
      version: String(pkg.version || ''),
      enabled: value(state.enabled) === true,
      running: String(value(state.status) || '') === 'STARTED',
      audioPath: info.has_alsa_contribution === true
    });
  });
  out.sort(function (a, b) {
    if (a.audioPath !== b.audioPath) return a.audioPath ? -1 : 1;
    return a.title.toLowerCase() < b.title.toLowerCase() ? -1 : a.title.toLowerCase() > b.title.toLowerCase() ? 1 : 0;
  });
  return out;
}

module.exports = { memoryOf, pluginsOf };
