'use strict';

// The capture behind a report: while the user reproduces a problem Glass
// logs in full, and when they say it has happened the player's system log
// is sent through Volumio's own submitter and its link comes back. The log
// level is raised for the capture only and put back by this side, never by
// the page: on the finish, on a cancel, when the time runs out, and at the
// next start when the backend went down in between. A marker on disk holds
// the level to go back to, so a restart in the middle of a capture, which
// is how some problems are reproduced, neither loses the capture nor
// leaves the level raised.

const LIMIT_MS = 10 * 60 * 1000;
const REPLY_WAIT_MS = 120 * 1000;
const RAISED = { level: 'verbose', targets: [] };
const MAX_TEXT = 2000;

class CaptureError extends Error {
  constructor(code) { super(code); this.code = code; }
}

// What the submitter said: the log server's answer with the link, or the
// words it prints when the log could not be sent and was kept on the
// player.
function parseReply(reply) {
  const text = String(reply || '').trim();
  const start = text.indexOf('{');
  if (start !== -1) {
    try {
      const answer = JSON.parse(text.slice(start, text.lastIndexOf('}') + 1));
      if (answer && answer.status === 'OK' && /^https?:\/\/\S+$/.test(String(answer.link || ''))) return { link: String(answer.link) };
    } catch (e) { /* not the server's answer */ }
  }
  return { error: text ? 'not-sent' : 'no-reply', said: text.split('\n')[0].slice(0, 200) };
}

// The line the log is filed under on Volumio's side: one line of plain
// text, the version and the symptom first.
function describe(version, symptom, text) {
  const said = String(text || '').replace(/\s+/g, ' ').trim().slice(0, 300);
  return 'Glass ' + version + ' report: ' + symptom + (said ? ' - ' + said : '');
}

class Capture {
  // deps: file (the marker's path), now(), read(path), write(path, text),
  // remove(path), logSettings(), setLogSettings(s), relaunch(),
  // sendLog(description) resolving with the submitter's words, and
  // setTimer(fn, ms) / clearTimer(handle).
  constructor(deps) {
    this.deps = Object.assign({ setTimer: setTimeout, clearTimer: clearTimeout, limitMs: LIMIT_MS, replyWaitMs: REPLY_WAIT_MS }, deps);
    this.marker = null;
    this.timer = null;
    this.sending = false;
  }

  // At start: a capture the backend went down under goes on until its
  // time, or ends now when the time has passed.
  resume() {
    let marker = null;
    try { marker = JSON.parse(this.deps.read(this.deps.file)); } catch (e) { marker = null; }
    if (!marker || !marker.previous) return this.view();
    this.marker = marker;
    if (this.deps.now() >= marker.until) this.restore();
    else this.arm(marker.until - this.deps.now());
    return this.view();
  }

  arm(ms) {
    const self = this;
    if (this.timer) this.deps.clearTimer(this.timer);
    this.timer = this.deps.setTimer(function () { self.timer = null; if (!self.sending) self.restore(); }, ms);
    if (this.timer && typeof this.timer.unref === 'function') this.timer.unref();
  }

  // The level back as it was, the marker gone.
  restore() {
    if (this.timer) { this.deps.clearTimer(this.timer); this.timer = null; }
    const marker = this.marker;
    this.marker = null;
    if (marker && marker.previous) {
      try { this.deps.setLogSettings(marker.previous); } catch (e) { /* the settings are not writable; the marker goes all the same */ }
    }
    try { this.deps.remove(this.deps.file); } catch (e) { /* none there */ }
  }

  view() {
    if (!this.marker) return { state: 'idle' };
    return { state: this.sending ? 'sending' : 'capturing', symptom: this.marker.symptom, startedAt: this.marker.startedAt, until: this.marker.until };
  }

  // Begin: remember the level, raise it, and have the display start again
  // so its own lines are full too.
  start(symptom, diagnosis) {
    if (this.marker) throw new CaptureError('capturing');
    const now = this.deps.now();
    const marker = { symptom, startedAt: now, until: now + this.deps.limitMs, previous: this.deps.logSettings(), diagnosis: diagnosis || null };
    this.deps.write(this.deps.file, JSON.stringify(marker));
    this.marker = marker;
    this.deps.setLogSettings(RAISED);
    try { this.deps.relaunch(); } catch (e) { /* no display to start again */ }
    this.arm(this.deps.limitMs);
    return this.view();
  }

  cancel() {
    if (this.sending) throw new CaptureError('sending');
    this.restore();
    return this.view();
  }

  // It has happened: the system log goes out while the level is still
  // raised, then the level goes back whatever the sending came to.
  async finish(version, text) {
    if (!this.marker) throw new CaptureError('not-capturing');
    if (this.sending) throw new CaptureError('sending');
    const marker = this.marker;
    const said = String(text || '').trim().slice(0, MAX_TEXT);
    const endedAt = this.deps.now();
    this.sending = true;
    let log;
    let wait = null;
    try {
      const reply = await Promise.race([
        this.deps.sendLog(describe(version, marker.symptom, said)),
        new Promise((resolve) => {
          wait = this.deps.setTimer(() => resolve(''), this.deps.replyWaitMs);
          if (wait && typeof wait.unref === 'function') wait.unref();
        })
      ]);
      log = parseReply(reply);
    } catch (e) {
      log = { error: 'not-sent', said: String(e && e.message ? e.message : e).slice(0, 200) };
    } finally {
      if (wait) this.deps.clearTimer(wait);
      this.sending = false;
      this.restore();
    }
    return { symptom: marker.symptom, startedAt: marker.startedAt, endedAt, text: said, diagnosis: marker.diagnosis, log };
  }
}

module.exports = { Capture, CaptureError, parseReply, describe, LIMIT_MS, RAISED };
