'use strict';
// Back to the stable release: what the user is asked, part by part, and
// what the answers make of the player's settings.
//
// The act puts the latest release that is not a test release in place, of
// Glass and of glass-evo where it is here, and the settings back to what
// that release comes with. Before it does, it asks of each part of the
// settings whether it is kept, yes or no, with an answer suggested: what
// describes the player's hardware and its place on the network is kept
// unless the user says no, what is a preference or a tuning goes back to
// the default unless the user says yes. What the user brought as files
// (themes, fonts, looks, backups) is never touched, and neither are a few
// keys that are not preferences at all.
//
// The questions and the plan are pure: the settings come in as objects and
// go out as objects. `run` is the act itself, in the order that changes
// nothing before what it needs is here and checked; everything it touches
// is handed to it, so it is tried without a player.

const { UpdateError, compareVersions } = require('./update');

// The parts in the order they are asked. `keep` is the suggested answer;
// `when` names what must hold for the question to be asked at all.
const PARTS = [
  { id: 'screen', keep: true },
  { id: 'network', keep: true },
  { id: 'owner', keep: false, when: 'holds' },
  { id: 'show', keep: false },
  { id: 'look', keep: false, when: 'evo' },
  { id: 'behaviour', keep: false },
  { id: 'tuning', keep: false },
  { id: 'tests', keep: false, when: 'tests' }
];

// Two classes beside the parts: `always` is kept whatever is answered,
// `default` goes back to the default whatever is answered.
const ALWAYS = 'always';
const DEFAULT = 'default';

// Which part a key belongs to, by its name or by a name that ends in a
// star for everything that begins so. The first that fits decides.
// The plugin's own configuration (config.json).
const CONFIG = [
  ['displayOutput', 'screen'], ['headless', 'screen'],
  ['remotesEnabled', 'network'], ['remoteBeaconPort', 'network'], ['remoteChannelPort', 'network'], ['remoteFramesPort', 'network'],
  ['managerHost', 'network'], ['managerPort', 'network'], ['smbShareAccess', 'network'],
  ['activeFolder', 'show'], ['activeFolder_title', 'show'], ['randomSelection', 'show'], ['themeTagRules', 'show'],
  // The key is the user's own, brought like a file; the show's settings are preferences.
  ['fanart_personal_key', ALWAYS], ['fanart*', 'show'],
  ['viewsFace', 'look'],
  ['timeout', 'behaviour'], ['persist_duration', 'behaviour'], ['persist_display', 'behaviour'], ['carDash*', 'behaviour'],
  ['animation', 'tuning'], ['frameRate', 'tuning'], ['rotationFPS', 'tuning'], ['rotationQuality', 'tuning'], ['transitionType', 'tuning'],
  ['perfProfile', 'tuning'], ['logLevel', 'tuning'], ['logTargets', 'tuning'],
  ['testReleases', 'tests'],
  // Whether an uninstall leaves the themes, and marks of things done once.
  ['doNotDeleteThemes', ALWAYS], ['legacyImported', ALWAYS], ['sideOutputsRetired', ALWAYS]
];
// The meter configuration's [current].
const METER = [
  ['screen.*', 'screen'], ['touch.matrix', 'screen'], ['touch.mapping', 'screen'], ['touch.calibrated', 'screen'], ['touch.swap', 'screen'], ['touch.flip.*', 'screen'], ['position.*', 'screen'],
  ['remote.*', 'network'],
  ['meter', 'show'], ['meter.folder', 'show'], ['random.*', 'show'], ['font.*', 'show'], ['use.system.fonts', 'show'], ['playinfo.type.mode', 'show'], ['queue.mode', 'show'],
  // The board's word on frost is the plugin's, noted again at every start.
  ['face.frost.suits', DEFAULT], ['face.*', 'look'],
  ['exit.on.touch', 'behaviour'], ['stop.display.on.touch', 'behaviour'], ['touch.interactive', 'behaviour'],
  ['frame.rate', 'tuning'], ['frame.rate.*', 'tuning'], ['rotation.*', 'tuning'], ['reel.direction', 'tuning'], ['spool.*', 'tuning'], ['scrolling.*', 'tuning'],
  ['start.animation', 'tuning'], ['transition.*', 'tuning'], ['debug.*', 'tuning'], ['use.logging', 'tuning'], ['use.cache', 'tuning'], ['cache.size', 'tuning'], ['color.depth', 'tuning'],
  // Where the themes are and what the display is for: the release's own.
  ['base.folder', DEFAULT], ['output.*', DEFAULT]
];
// The spectrum configuration's [current]: the theme on show is the user's
// choice, everything else there is the release's own.
const SPECTRUM = [
  ['spectrum', 'show'], ['spectrum.folder', 'show'],
  ['base.folder', DEFAULT], ['update.period', DEFAULT], ['max.value', DEFAULT], ['pipe.name', DEFAULT], ['size', DEFAULT], ['update.ui.interval', DEFAULT],
  ['frame.rate', DEFAULT], ['depth', DEFAULT], ['exit.on.touch', DEFAULT], ['use.logging', DEFAULT], ['use.test.data', DEFAULT]
];
// The sections of either file other than [current], as a whole, by the
// first word of their name: the screen's environment goes with the screen;
// where the meter's data comes from and the side outputs of old are the
// release's own.
const SECTIONS = [['sdl', 'screen'], ['data', DEFAULT], ['serial', DEFAULT], ['i2c', DEFAULT], ['pwm', DEFAULT], ['http', DEFAULT], ['web', DEFAULT]];

const TABLES = { config: CONFIG, meter: METER, spectrum: SPECTRUM, section: SECTIONS };

// The class of a key in one of the tables, or null for a key no table
// names: such a key goes back to the default, as a setting a later release
// wrote and the stable one does not know.
function classOf(table, key) {
  const rows = TABLES[table] || [];
  for (const row of rows) {
    const name = row[0];
    if (name === key || (name[name.length - 1] === '*' && key.indexOf(name.slice(0, -1)) === 0)) return row[1];
  }
  return null;
}

// The questions for a player: the parts that apply, each with its
// suggested answer. `facts`: `holds` (glass-evo holds the screen), `evo`
// (glass-evo is here), `tests` (the player takes test releases).
function questions(facts) {
  const f = facts || {};
  return PARTS.filter(function (part) { return !part.when || f[part.when] === true; })
    .map(function (part) { return { id: part.id, keep: part.keep }; });
}

// The answers as given, each part's suggested answer where none was given
// or the given one is no yes or no; a part that does not apply is not kept.
function answers(given, facts) {
  const out = {};
  const asked = questions(facts);
  PARTS.forEach(function (part) { out[part.id] = false; });
  asked.forEach(function (part) {
    const said = given && typeof given[part.id] === 'boolean' ? given[part.id] : part.keep;
    out[part.id] = said;
  });
  return out;
}

function copy(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

// One flat group of keys: the defaults, and over them what is kept of the
// current ones.
function laid(table, current, defaults, keep) {
  const out = copy(defaults) || {};
  Object.keys(current || {}).forEach(function (key) {
    const cls = classOf(table, key);
    if (cls === ALWAYS || (cls && cls !== DEFAULT && keep[cls] === true)) out[key] = copy(current[key]);
  });
  return out;
}

// A configuration file with sections: [current] key by key, every other
// section as a whole.
function laidFile(table, current, defaults, keep) {
  const now = current || {};
  const base = defaults || {};
  const out = {};
  Object.keys(base).forEach(function (section) {
    if (section !== 'current') out[section] = copy(base[section]);
  });
  Object.keys(now).forEach(function (section) {
    if (section === 'current') return;
    const cls = classOf('section', section);
    if (cls === ALWAYS || (cls && cls !== DEFAULT && keep[cls] === true)) out[section] = copy(now[section]);
  });
  out.current = laid(table, now.current, base.current, keep);
  // [current] first, as the files are written.
  const ordered = { current: out.current };
  Object.keys(out).forEach(function (section) { if (section !== 'current') ordered[section] = out[section]; });
  return ordered;
}

// The settings after the act: `current` and `defaults` each hold `config`
// (the plugin's configuration, key to entry), `meter` and `spectrum` (the
// two configuration files, section to keys); `keep` is the answers.
function plan(current, defaults, keep) {
  const c = current || {};
  const d = defaults || {};
  const k = keep || {};
  return {
    config: laid('config', c.config, d.config, k),
    meter: laidFile('meter', c.meter, d.meter, k),
    spectrum: laidFile('spectrum', c.spectrum, d.spectrum, k)
  };
}

// How many settings a plan changes, for the log: keys that differ between
// the current settings and the planned ones, in the three files together.
function changed(current, planned) {
  let n = 0;
  const count = function (a, b) {
    const keys = Object.keys(Object.assign({}, a || {}, b || {}));
    keys.forEach(function (key) { if (JSON.stringify((a || {})[key]) !== JSON.stringify((b || {})[key])) n++; });
  };
  const c = current || {};
  count(c.config, planned.config);
  ['meter', 'spectrum'].forEach(function (file) {
    const sections = Object.keys(Object.assign({}, c[file] || {}, planned[file] || {}));
    sections.forEach(function (section) {
      if (section === 'current') count((c[file] || {}).current, (planned[file] || {}).current);
      else if (JSON.stringify((c[file] || {})[section]) !== JSON.stringify((planned[file] || {})[section])) n++;
    });
  });
  return n;
}

// What the act does about each of the two releases: nothing where the
// stable one is installed, else the step to it, forward or back.
// `installed` and `stable` are versions, `installed` null where the thing
// is not here; `compare` is the versions' comparison.
function step(installed, stable, compare) {
  if (!installed || !stable) return { action: 'none', from: installed || null, to: null };
  const c = compare(stable, installed);
  return { action: c === 0 ? 'none' : c < 0 ? 'back' : 'forward', from: installed, to: stable };
}

// The act, with the user's answers. In this order: the stable Glass's zip
// staged where Glass is another version; the settings backed up; glass-evo
// stepped to its stable release, held to the Glass that will be here; the
// screen given back to the kiosk unless kept; the settings laid as
// answered over what the stable release comes with; Glass replaced, or,
// where it is the stable release already, the backend restarted so the
// plugin starts on the settings. A step that fails puts the settings and
// glass-evo back as they were and is thrown on; the screen, once given
// back, stays with the kiosk.
//
// `with_`: `updater` and `component` (the Manager's own), `plugin` (the
// plugin's methods: stableFacts, stableCurrent, stableDefaults,
// stableWrite, backupCreate, backupRestore, setScreenOwner,
// componentChanged, restartBackend), `texts` (a staged zip's own files:
// package, config, meter, spectrum), `least` (the least glass-evo a
// package names), `logger`. Answers with what was done.
async function run(job, keep, with_) {
  const updater = with_.updater;
  const component = with_.component;
  const plugin = with_.plugin;
  const logger = with_.logger || console;
  let moved = false;
  let backup = null;
  let written = false;
  try {
    job.state = 'verifying';
    const glass = await updater.stable();
    const here = component.installed();
    const evo = here.installed ? await component.stable() : null;
    const glassStep = step(updater.version, glass.version, compareVersions);
    const evoStep = step(here.installed ? here.version : null, evo ? evo.version : null, compareVersions);
    const staged = glassStep.action === 'none' ? null : await updater.stage(job, glass);
    // What the stable release comes with: from its zip, or from the plugin
    // installed here where that is the stable release.
    const texts = staged ? await with_.texts(staged.file) : {};
    const defaults = plugin.stableDefaults(texts);
    const least = staged ? with_.least(texts.package || '') : component.least;
    job.state = 'backing-up';
    const made = plugin.backupCreate(updater.backupName('stable'), { automatic: true });
    if (!made || made.error) throw new UpdateError('backup', 'the settings could not be backed up (' + ((made && made.error) || 'no answer') + '); nothing was changed');
    backup = made.name;
    if (evoStep.action !== 'none') {
      await component.installRelease(job, evo, { glass: glass.version, least: least });
      moved = true;
    }
    job.state = 'applying';
    if (plugin.stableFacts().holds && !keep.owner) {
      const back = await plugin.setScreenOwner('kiosk');
      if (back && back.error) throw new UpdateError('owner', 'the screen could not be given back to the kiosk (' + (back.message || back.error) + ')');
    }
    const current = plugin.stableCurrent();
    const planned = plan(current, defaults, keep);
    // Said before the writing: one that breaks half way is put back too.
    written = true;
    plugin.stableWrite(planned);
    const kept = Object.keys(keep).filter(function (k) { return keep[k]; });
    logger.info('glass: manager stable: ' + changed(current, planned) + ' settings changed; kept: ' + (kept.join(', ') || 'nothing') + '; backup ' + backup);
    if (staged) {
      await updater.apply(job, staged, { backup: backup });
    } else {
      job.state = 'restarting';
      job.target = updater.version;
      plugin.restartBackend();
    }
    logger.info('glass: manager stable: Glass ' + (staged ? updater.version + ' to ' + glass.version : updater.version + ' stays') + (evoStep.action !== 'none' ? ', glass-evo ' + evoStep.from + ' to ' + evoStep.to : '') + '; the backend restarts');
    return { glass: glassStep, evo: evoStep, backup: backup, kept: kept };
  } catch (e) {
    if (written && backup) {
      const restored = plugin.backupRestore(backup);
      if (restored && restored.error) logger.warn('glass: manager stable: the settings were not put back (' + restored.error + '); the backup ' + backup + ' holds them');
    }
    if (moved) {
      try {
        await component.rollback();
        plugin.componentChanged();
      } catch (e2) {
        logger.warn('glass: manager stable: glass-evo not put back: ' + e2.message);
      }
    }
    throw e;
  }
}

module.exports = { run: run, PARTS: PARTS, ALWAYS: ALWAYS, DEFAULT: DEFAULT, TABLES: TABLES, classOf: classOf, questions: questions, answers: answers, plan: plan, changed: changed, step: step };
