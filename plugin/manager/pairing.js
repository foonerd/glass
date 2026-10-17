'use strict';

// The file path's pairing rule, whole and pure: what to do when a zip comes
// by hand, given what runs on the player, what is kept from an earlier
// upload, and what the player can see online. The routes act on the plan
// and decide nothing themselves; the test replays every row of the table
// in the journal's design note.

const { compareVersions } = require('./update');

const before = (a, b) => compareVersions(a, b) < 0;

// Whether a glass-evo (its version and the Glass it requires) goes with a
// Glass (its version and the least glass-evo it names).
function fits(evo, glass) {
  if (!evo || !glass) return false;
  if (glass.least && before(evo.version, glass.least)) return false;
  if (evo.requires && before(glass.version, evo.requires)) return false;
  return true;
}

// `p`: `glass` the version running, `evo` the glass-evo installed or null,
// `least` the glass-evo the running Glass names as its least, `zip` what
// came ({kind: 'glass', version, least} or {kind: 'evo', version,
// requires}), `held` the kept zip of the other kind or null (one of the
// same kind is not the rule's business: the newer replaces it), `online`
// the latest versions the player can see ({glass, evo}) or null.
//
// The plan: `action` is one of nothing, install-glass, install-evo,
// install-both, install-held, install-glass-fetching-evo, hold-glass,
// hold-evo, refuse-too-old; `say` names the words; `dropped` is the held
// zip of the other kind that is discarded by this plan, else null;
// `needs` the version a held zip waits for; `least` the running Glass's
// least on a refusal; `online` on hold-evo whether a Glass that would
// take it is released.
function plan(p) {
  const running = { version: p.glass, least: p.least || null };
  const installed = p.evo ? { version: p.evo, requires: null } : null;
  const zip = p.zip;
  const held = p.held && p.held.kind !== zip.kind ? p.held : null;
  const online = p.online || {};
  const out = (action, say, more) => Object.assign({ action: action, say: say, dropped: null }, more || {});
  if (zip.kind === 'glass') {
    const glass = { version: zip.version, least: zip.least || null };
    if (zip.version === p.glass) {
      // The version running: nothing, unless a kept glass-evo that fits
      // the running Glass goes in by itself; one that is what is installed
      // already is moot.
      const moot = !!(held && installed && held.version === installed.version);
      if (held && !moot && fits(held, running)) return out('install-held', 'held-evo-in');
      return out('nothing', 'same-version', { dropped: moot ? held : null });
    }
    // glass-evo is an extra: a player without one needs none, as the
    // online upgrade has it.
    const needsEvo = !!(installed && glass.least && before(installed.version, glass.least));
    if (held && fits(held, glass)) return out('install-both', 'both-in');
    if (!needsEvo) return out('install-glass', 'glass-in', { dropped: held });
    if (online.evo && !before(online.evo, glass.least)) return out('install-glass-fetching-evo', 'glass-in-fetching-evo', { dropped: held });
    return out('hold-glass', 'glass-held', { needs: glass.least, dropped: held });
  }
  const evo = { version: zip.version, requires: zip.requires || null };
  if (installed && zip.version === installed.version) {
    if (held && fits(evo, held)) return out('install-both', 'both-in');
    return out('nothing', 'same-version');
  }
  if (held && fits(evo, held)) return out('install-both', 'both-in');
  if (running.least && before(evo.version, running.least)) return out('refuse-too-old', 'evo-too-old', { least: running.least });
  const needsGlass = !!(evo.requires && before(running.version, evo.requires));
  if (!needsGlass) return out('install-evo', 'evo-in', { dropped: held });
  return out('hold-evo', 'evo-held', { needs: evo.requires, online: !!(online.glass && !before(online.glass, evo.requires)), dropped: held });
}

module.exports = { plan, fits };
