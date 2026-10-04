'use strict';

// The guided diagnosis: for the symptom a user names, the checks the
// Manager can run itself over facts it already has, the status sheet, the
// screen's facts, the release state and Glass's last lines in the journal.
// Every check is pure, facts in and a finding or nothing out, and stands
// for one cause the Troubleshooting page knows. A finding names its check,
// its kind (a likely cause, something worth knowing, or a thing checked
// and found right), the string that says it with the values to fill in,
// and the Manager's tab where it is set, when there is one.

const SYMPTOMS = ['no-meters', 'not-moving', 'restarts', 'touch', 'screen', 'artwork', 'remotes', 'slow', 'other'];

const cause = (check, key, fill, go) => ({ check, kind: 'cause', key, with: fill || {}, go: go || null });
const note = (check, key, fill, go) => ({ check, kind: 'note', key, with: fill || {}, go: go || null });
const ok = (check, key, fill) => ({ check, kind: 'ok', key, with: fill || {}, go: null });

// A journal line's time: the short-iso stamp it opens with.
function lineTime(line) {
  const m = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})([+-]\d{2}):?(\d{2})/.exec(String(line));
  return m ? Date.parse(m[1] + m[2] + ':' + m[3]) : NaN;
}

// Glass's lines since the backend last started: what an earlier run said
// is no evidence about this one.
function linesOfThisRun(f) {
  const up = Number(f.status && f.status.sheet && f.status.sheet.player && f.status.sheet.player.backendUptimeS);
  const since = f.now && up >= 0 ? f.now - up * 1000 : NaN;
  return (f.log || []).filter(function (line) {
    const at = lineTime(line);
    return isNaN(since) || isNaN(at) || at >= since - 1000;
  });
}

const playing = (f) => f.status.channel && f.status.channel.status === 'play';
const megabytes = (bytes) => Math.round(bytes / 1048576);

const CHECKS = [
  // ---- for every symptom ---------------------------------------------
  {
    id: 'release', symptoms: SYMPTOMS,
    run(f) {
      const u = f.update;
      if (!u || !u.latest || !u.latest.version) return null;
      return u.available ? note('release', 'DIAG_RELEASE_NEWER', { current: u.current, latest: u.latest.version }, 'system')
        : ok('release', 'DIAG_RELEASE_CURRENT', { current: u.current });
    }
  },
  {
    id: 'legacy', symptoms: SYMPTOMS,
    run(f) { return f.status.legacy ? cause('legacy', 'DIAG_LEGACY_ON') : null; }
  },
  {
    id: 'binary', symptoms: SYMPTOMS,
    run(f) { return f.status.binary === false ? cause('binary', 'DIAG_NO_BINARY', { arch: f.status.arch || '?' }) : null; }
  },
  {
    id: 'disk', symptoms: SYMPTOMS,
    run(f) {
      const free = f.status.diskFree;
      return typeof free === 'number' && free < 100 * 1048576 ? cause('disk', 'DIAG_DISK_FULL', { mb: megabytes(free) }) : null;
    }
  },
  {
    id: 'journal', symptoms: SYMPTOMS,
    run(f) {
      const house = f.status.sheet && f.status.sheet.housekeeping;
      const problems = house && Array.isArray(house.problems) ? house.problems : [];
      return problems.length ? note('journal', 'DIAG_JOURNAL_PROBLEMS', { lines: problems.join('\n') }, 'system') : null;
    }
  },

  // ---- the meters never appear ----------------------------------------
  {
    id: 'output', symptoms: ['no-meters'],
    run(f) { return f.status.headless ? cause('output', 'DIAG_HEADLESS') : null; }
  },
  {
    id: 'playing', symptoms: ['no-meters', 'not-moving'],
    run(f) {
      const status = (f.status.channel && f.status.channel.status) || '?';
      return playing(f) ? ok('playing', 'DIAG_PLAYING') : cause('playing', 'DIAG_NOT_PLAYING', { status, timeout: f.status.timeout });
    }
  },
  {
    id: 'somewhere', symptoms: ['no-meters', 'restarts', 'screen'],
    run(f) {
      const sc = f.screen;
      if (!sc || !sc.fact || !sc.now) return null;
      // Where glass-evo holds the screen the kiosk is not meant to run: its
      // unit standing as failed (a take before 0.8.18 left it so) is no cause.
      const evo = !!(sc.owner && sc.owner.owner === 'glass-evo');
      if (sc.fact.kiosk === 'failed' && !evo) return cause('somewhere', 'DIAG_KIOSK_FAILED');
      if (sc.now.wouldDraw || (sc.now.display && sc.now.display.running)) return null;
      return !sc.fact.panel && !sc.fact.xserver ? cause('somewhere', 'DIAG_NO_PANEL') : cause('somewhere', 'DIAG_KIOSK_ON_ITS_WAY', { kiosk: sc.fact.kiosk || '?' });
    }
  },
  {
    id: 'display', symptoms: ['no-meters'],
    run(f) {
      if (!playing(f) || f.status.headless) return null;
      return f.status.running ? ok('display', 'DIAG_DISPLAY_UP') : note('display', 'DIAG_DISPLAY_NOT_UP', { timeout: f.status.timeout });
    }
  },
  {
    id: 'deaths', symptoms: ['no-meters', 'restarts'],
    run(f) {
      const lines = linesOfThisRun(f);
      if (lines.some((l) => /missing or is the wrong CPU/.test(l))) return cause('deaths', 'DIAG_LOG_WRONG_BINARY', { arch: f.status.arch || '?' });
      const window = lines.filter((l) => /could not open the window/.test(l));
      if (window.length) return cause('deaths', 'DIAG_LOG_NO_WINDOW', { count: window.length });
      const died = lines.filter((l) => /the display died \d+ s after launch/.test(l));
      if (died.length) return cause('deaths', 'DIAG_LOG_DIED', { count: died.length, last: died[died.length - 1].replace(/^.*?glass:\s*/, '') }, 'system');
      return ok('deaths', 'DIAG_LOG_NO_DEATHS', { lines: lines.length });
    }
  },

  {
    // Whether the system's graphics can draw on a screen of the display's
    // own (the Manager's check: the display's probe, the kiosk's X log).
    id: 'graphics', symptoms: ['no-meters', 'restarts', 'screen'],
    run(f) {
      const g = f.screen && f.screen.graphics;
      if (!g || !g.asked) return null;
      if (g.applies && g.ok === false) return cause('graphics', 'DIAG_GRAPHICS_BROKEN', { reason: g.reason }, 'screen');
      if (g.x && g.x.state === 'software') return note('graphics', 'DIAG_GRAPHICS_X_SOFTWARE', { detail: g.x.detail });
      if (g.applies && g.ok && g.software) return note('graphics', 'DIAG_GRAPHICS_SOFTWARE', { renderer: g.renderer });
      return g.applies && g.ok ? ok('graphics', 'DIAG_GRAPHICS_OK', { reason: g.reason }) : null;
    }
  },

  // ---- the meters show but do not move ---------------------------------
  {
    id: 'tap', symptoms: ['not-moving', 'remotes'],
    run(f) {
      const audio = f.status.sheet && f.status.sheet.audio;
      if (!audio) return null;
      return audio.tapInChain ? ok('tap', 'DIAG_TAP_IN') : cause('tap', 'DIAG_TAP_OUT');
    }
  },
  {
    id: 'measured', symptoms: ['not-moving', 'remotes'],
    run(f) {
      if (!playing(f)) return null;
      return f.status.measured ? ok('measured', 'DIAG_MEASURED')
        : cause('measured', 'DIAG_NOT_MEASURED', { service: (f.status.channel && f.status.channel.service) || '?' });
    }
  },

  // ---- touch and the controls ------------------------------------------
  {
    id: 'interactive', symptoms: ['touch'],
    run(f) {
      const mode = f.status.interactive;
      if (mode === 'off') return cause('interactive', 'DIAG_INTERACTIVE_OFF', {}, 'system');
      if (mode === 'theme') return note('interactive', 'DIAG_INTERACTIVE_THEME', {}, 'system');
      return mode === 'on' ? ok('interactive', 'DIAG_INTERACTIVE_ON') : null;
    }
  },
  {
    id: 'pointing', symptoms: ['touch'],
    run(f) {
      const inputs = f.screen && f.screen.probe && f.screen.probe.inputs;
      if (!inputs) return null;
      const found = (inputs.touch || []).concat(inputs.mice || []);
      return found.length ? ok('pointing', 'DIAG_POINTING', { names: found.map((d) => d.name).join(', ') }) : cause('pointing', 'DIAG_NO_POINTING');
    }
  },
  {
    id: 'touch-map', symptoms: ['touch'],
    run(f) {
      const sc = f.screen;
      if (!sc || !sc.touch) return null;
      if (!sc.free) return note('touch-map', 'DIAG_TOUCH_UNDER_KIOSK');
      const c = sc.touch.calibration || {};
      if (c.state === 'failed') return cause('touch-map', 'DIAG_CALIBRATION_FAILED', {}, 'screen');
      return note('touch-map', sc.touch.mapping === 'auto' ? 'DIAG_TOUCH_MAP_AUTO' : 'DIAG_TOUCH_MAP_SET', { mapping: sc.touch.mapping }, 'screen');
    }
  },
  {
    id: 'channel', symptoms: ['touch'],
    run(f) {
      if (!f.status.running || !f.status.channel) return null;
      return f.status.channel.clients > 0 ? ok('channel', 'DIAG_CHANNEL_UP') : cause('channel', 'DIAG_CHANNEL_DOWN');
    }
  },

  // ---- the picture on the screen ---------------------------------------
  {
    id: 'whose', symptoms: ['screen'],
    run(f) {
      if (!f.screen) return null;
      return f.screen.free ? ok('whose', 'DIAG_SCREEN_GLASS') : note('whose', 'DIAG_SCREEN_KIOSK', {}, 'screen');
    }
  },
  {
    id: 'rotation', symptoms: ['screen', 'touch'],
    run(f) {
      const sc = f.screen;
      if (!sc || !sc.free || !sc.probe || !sc.probe.suggestion) return null;
      const suggested = sc.probe.suggestion.rotation || 0;
      const rotation = sc.rotation || 0;
      if (rotation === suggested) return ok('rotation', 'DIAG_ROTATION_AS_SUGGESTED', { rotation });
      const fill = { rotation, suggested, panel: sc.probe.suggestion.panel || '?' };
      return rotation === 0 ? cause('rotation', 'DIAG_ROTATION_PORTRAIT', fill, 'screen') : note('rotation', 'DIAG_ROTATION_DIFFERS', fill, 'screen');
    }
  },
  {
    id: 'size', symptoms: ['screen', 'slow'],
    run(f) {
      const screen = f.status.sheet && f.status.sheet.screen && f.status.sheet.screen.size;
      const theme = f.status.themeSize;
      if (!screen || !theme || screen === theme) return null;
      return note('size', 'DIAG_THEME_SIZE', { theme, screen }, 'artwork');
    }
  },

  // ---- pictures --------------------------------------------------------
  {
    id: 'fanart', symptoms: ['artwork', 'slow'],
    run(f) {
      const a = f.status.artwork;
      if (!a) return null;
      if (f.symptom === 'slow') return a.enabled ? note('fanart', 'DIAG_FANART_COSTS', {}, 'artwork') : null;
      return a.enabled ? ok('fanart', 'DIAG_FANART_ON') : cause('fanart', 'DIAG_FANART_OFF', {}, 'artwork');
    }
  },
  {
    id: 'places', symptoms: ['artwork'],
    run() { return note('places', 'DIAG_ARTWORK_PLACES'); }
  },

  // ---- remote displays, the Face, Anymote ------------------------------
  {
    id: 'serving', symptoms: ['remotes'],
    run(f) {
      const r = f.status.remotes;
      if (!r) return null;
      if (!r.enabled) return note('serving', 'DIAG_SERVING_OFF', {}, 'remotes');
      if (!r.serving || r.problem) return cause('serving', 'DIAG_SERVING_DOWN', { problem: r.problem || '-' }, 'remotes');
      return ok('serving', 'DIAG_SERVING', { frames: r.ports.frames, channel: r.ports.channel });
    }
  },
  {
    id: 'receiving', symptoms: ['remotes'],
    run(f) {
      const r = f.status.remotes;
      if (!r || !r.enabled || !r.serving) return null;
      if (r.connected === 0) return note('receiving', 'DIAG_NO_REMOTES', {}, 'remotes');
      return r.receiving < r.connected ? cause('receiving', 'DIAG_NOT_RECEIVING', { connected: r.connected, receiving: r.receiving, port: r.ports.frames }, 'remotes')
        : ok('receiving', 'DIAG_RECEIVING', { connected: r.connected });
    }
  },
  {
    id: 'face', symptoms: ['remotes'],
    run(f) {
      const face = f.status.face;
      if (!face || !face.pages) return null;
      if (face.frames) return ok('face', 'DIAG_FACE_FRAMES', { pages: face.pages });
      return playing(f) && f.status.measured ? cause('face', 'DIAG_FACE_NO_FRAMES') : note('face', 'DIAG_FACE_RESTS');
    }
  },

  // ---- it is slow ------------------------------------------------------
  {
    id: 'governed', symptoms: ['slow', 'restarts'],
    run(f) {
      const p = f.status.performance;
      const rate = f.status.showing && f.status.showing.rate;
      if (!p || !p.values || !rate || !f.status.running) return null;
      return rate < p.values.frameRate ? cause('governed', 'DIAG_GOVERNED', { rate, set: p.values.frameRate }, 'system') : null;
    }
  },
  {
    id: 'rate', symptoms: ['slow'],
    run(f) {
      const p = f.status.performance;
      if (!p || !p.values || !p.profiles || !p.profiles[p.auto]) return null;
      const suits = p.profiles[p.auto].frameRate;
      const board = (p.board && p.board.model) || '?';
      return p.values.frameRate > suits ? note('rate', 'DIAG_RATE_ABOVE_BOARD', { rate: p.values.frameRate, suits, board }, 'system')
        : ok('rate', 'DIAG_RATE_SUITS', { rate: p.values.frameRate, board });
    }
  }
];

// The findings for a symptom: likely causes first, then what is worth
// knowing, then what was checked and found right; and whether any cause
// was found. A check that throws on a fact it did not expect says nothing.
function diagnose(symptom, facts) {
  if (SYMPTOMS.indexOf(symptom) === -1) throw new Error('unknown symptom');
  const f = Object.assign({ status: {}, screen: null, update: null, log: [], now: Date.now() }, facts, { symptom });
  const findings = [];
  CHECKS.forEach(function (check) {
    if (check.symptoms.indexOf(symptom) === -1) return;
    let found = null;
    try { found = check.run(f); } catch (e) { found = null; }
    if (found) findings.push(found);
  });
  const order = { cause: 0, note: 1, ok: 2 };
  findings.sort((a, b) => order[a.kind] - order[b.kind]);
  return { symptom, found: findings.some((x) => x.kind === 'cause'), findings };
}

module.exports = { SYMPTOMS, CHECKS, diagnose, lineTime };
