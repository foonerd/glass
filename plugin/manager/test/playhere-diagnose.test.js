'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { SYMPTOMS, CHECKS } = require('../diagnose');

const check = CHECKS.find((c) => c.id === 'play-here');
const LANGUAGES = fs.readdirSync(path.join(__dirname, '../../i18n'))
  .map((f) => (f.match(/^strings_([a-z]+)\.json$/) || [])[1]).filter(Boolean).sort();

function facts(playHere, extra) {
  return Object.assign({
    now: Date.now(),
    status: Object.assign({
      channel: { status: 'play', service: 'mpd', clients: 1 },
      rings: [{ name: 'evoaudio.glass.100.1', tag: 'glass', audio: true, live: true, rate: 44100, channels: 2 }],
      playHere: Object.assign({ on: true, cap: 0, available: true, running: true, listeners: 1, max: 8, error: null, left: null,
        segment: { rate: 44100, channels: 2, format: 's16le', bits: 16, frame_bytes: 4, one_bit: false, capped: false, chunk_ms: 20 } }, playHere)
    }, extra || {})
  });
}

test('the symptom is offered and the check belongs to it', () => {
  assert.ok(SYMPTOMS.includes('play-here'));
  assert.ok(check, 'the play-here check exists');
  assert.ok(check.symptoms.includes('play-here') && check.symptoms.includes('other'));
});

test('the sound served: an ok with the listeners and the shape', () => {
  const r = check.run(facts({}));
  assert.strictEqual(r.kind, 'ok');
  assert.strictEqual(r.key, 'DIAG_PLAY_HERE_OK');
  assert.strictEqual(r.with.listeners, 1);
  assert.strictEqual(r.with.shape, '44100 Hz, 16-bit, 2 ch');
});

test('off is a note pointing at the System tab, before anything else', () => {
  const r = check.run(facts({ on: false, available: false, error: 'x' }));
  assert.strictEqual(r.kind, 'note');
  assert.strictEqual(r.key, 'DIAG_PLAY_HERE_OFF');
  assert.strictEqual(r.go, 'system');
});

test('the lite missing from the build, and the lite that failed, are causes', () => {
  assert.strictEqual(check.run(facts({ available: false })).key, 'DIAG_PLAY_HERE_MISSING');
  const failed = check.run(facts({ error: 'the lite left with code 3' }));
  assert.strictEqual(failed.kind, 'cause');
  assert.strictEqual(failed.key, 'DIAG_PLAY_HERE_LITE');
  assert.strictEqual(failed.with.error, 'the lite left with code 3');
});

test('the player playing with no live audio ring of the tap is the cause; a stopped player is not', () => {
  const none = check.run(facts({}, { rings: [{ name: 'evometer.glass.100.1', tag: 'glass', audio: false, live: true }] }));
  assert.strictEqual(none.key, 'DIAG_PLAY_HERE_NO_RING');
  const stale = check.run(facts({}, { rings: [{ name: 'evoaudio.glass.90.1', tag: 'glass', audio: true, live: false }] }));
  assert.strictEqual(stale.key, 'DIAG_PLAY_HERE_NO_RING');
  const other = check.run(facts({}, { rings: [{ name: 'evoaudio.other.100.1', tag: 'other', audio: true, live: true }] }));
  assert.strictEqual(other.key, 'DIAG_PLAY_HERE_NO_RING');
  const stopped = check.run(facts({ running: false, segment: null }, { channel: { status: 'stop' }, rings: [] }));
  assert.strictEqual(stopped, null, 'nothing to say of a player standing still with nobody listening');
});

test('a one-bit stream and a full house are notes', () => {
  const dsd = check.run(facts({ segment: { rate: 2822400, channels: 2, format: 's32le', bits: 32, frame_bytes: 8, one_bit: true } }));
  assert.strictEqual(dsd.kind, 'note');
  assert.strictEqual(dsd.key, 'DIAG_PLAY_HERE_DSD');
  const full = check.run(facts({ listeners: 8 }));
  assert.strictEqual(full.key, 'DIAG_PLAY_HERE_FULL');
  assert.strictEqual(full.with.max, 8);
});

test('the words exist in every language, with their holes', () => {
  for (const lang of LANGUAGES) {
    const s = require('../../i18n/strings_' + lang + '.json').GLASS;
    for (const key of ['DIAG_PLAY_HERE_OFF', 'DIAG_PLAY_HERE_MISSING', 'DIAG_PLAY_HERE_DSD']) assert.ok(s['MANAGER_' + key], lang + ' ' + key);
    assert.ok(s.MANAGER_DIAG_PLAY_HERE_LITE.indexOf('{error}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_PLAY_HERE_FULL.indexOf('{max}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_PLAY_HERE_OK.indexOf('{listeners}') !== -1 && s.MANAGER_DIAG_PLAY_HERE_OK.indexOf('{shape}') !== -1, lang);
    assert.ok(s.MANAGER_DIAG_PLAY_HERE_NO_RING.indexOf('`audio 1`') !== -1, lang + ' names the snippet word');
    assert.ok(s.MANAGER_DIAG_S_PLAY_HERE, lang + ' names the symptom');
    assert.ok(s.MANAGER_PLAY_HERE_RATE.indexOf('{rate}') !== -1, lang);
    assert.ok(s.MANAGER_ST_PLAY_HERE_ON.indexOf('{listeners}') !== -1 && s.MANAGER_ST_PLAY_HERE_ON.indexOf('{shape}') !== -1, lang);
  }
});
