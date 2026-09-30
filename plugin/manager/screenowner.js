'use strict';
// Who owns the player's screen, and the plan to change that. The kiosk is
// the player's interface on the screen until glass-evo replaces it; when
// glass-evo is chosen, everything that draws on the screen must vacate,
// through its own lifecycle, and everything changed is recorded so that
// the way back restores exactly what was changed and nothing else. The
// planners here are pure: they take what was found and answer with the
// ordered steps; the plugin runs the steps and keeps the register.

// The plugins that bring a kiosk, in the order they must go: Now Playing
// rides inside the kiosk's browser and rewrites its start script, so it
// goes first; the Touch Display plugin runs the kiosk on a Pi; the Display
// Configuration plugin configures X and restarts the kiosk on x86.
const KIOSK_PLUGINS = ['now_playing', 'touch_display', 'display_configuration'];

// The units behind the kiosk: the kiosk unit an image ships enabled at
// boot, and the console on the first terminal, which would show through
// an empty screen.
const KIOSK_UNITS = ['volumio-kiosk', 'getty@tty1'];

function running(status) {
  return status === 'STARTED' || status === 'STARTING';
}

// The plugin steps of a take: each kiosk plugin that is installed and
// running or enabled goes off, with what it was recorded.
function planTakePlugins(plugins) {
  const steps = [];
  KIOSK_PLUGINS.forEach(function (name) {
    const p = (plugins || {})[name];
    if (!p || !p.installed) return;
    if (!p.enabled && !running(p.status)) return;
    steps.push({ kind: 'plugin', category: 'user_interface', name: name, action: 'off', was: { enabled: !!p.enabled, status: String(p.status || '') } });
  });
  return steps;
}

// The unit steps of a take, read after the plugin steps ran (a plugin's
// own onStop may start the console again): a unit running is stopped, a
// unit enabled at boot is disabled, with what it was recorded.
function planTakeUnits(units) {
  const steps = [];
  KIOSK_UNITS.forEach(function (name) {
    const u = (units || {})[name];
    if (!u) return;
    const active = u.state === 'active' || u.state === 'activating' || u.state === 'reloading';
    if (!active && !u.enabled) return;
    steps.push({ kind: 'unit', name: name, action: 'stop', was: { active: active, enabled: !!u.enabled } });
  });
  return steps;
}

// The way back: the take's steps in reverse, each only where the thing is
// still as the take left it. A plugin the user turned on again in the
// meantime is left alone; one the user left off stays off if it was not
// the take that turned it off.
function planGiveBack(register, now) {
  const changes = (register && Array.isArray(register.changes)) ? register.changes.slice().reverse() : [];
  const steps = [];
  changes.forEach(function (c) {
    if (c.kind === 'plugin') {
      const p = ((now || {}).plugins || {})[c.name];
      if (!p || !p.installed) return;
      if (p.enabled || running(p.status)) return; // turned on again by someone else
      if (!c.was || !c.was.enabled) return; // was off before the take too
      steps.push({ kind: 'plugin', category: c.category || 'user_interface', name: c.name, action: 'on' });
    } else if (c.kind === 'unit') {
      const u = ((now || {}).units || {})[c.name];
      if (!u) return;
      const active = u.state === 'active' || u.state === 'activating';
      if (active || u.enabled) return; // running or enabled again by someone else
      if (!c.was) return;
      if (!c.was.active && !c.was.enabled) return;
      steps.push({ kind: 'unit', name: c.name, action: 'start', enable: !!c.was.enabled, start: !!c.was.active });
    }
  });
  return steps;
}

// Whether a register says glass-evo owns the screen now.
function ownedByEvo(register) {
  return !!(register && register.owner === 'glass-evo' && !register.gaveBackAt);
}

module.exports = { KIOSK_PLUGINS, KIOSK_UNITS, planTakePlugins, planTakeUnits, planGiveBack, ownedByEvo };
