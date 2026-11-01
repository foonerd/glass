'use strict';

const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { readRings } = require('../rings');

// A measurement ring's header as the tap writes it (crates/tap/src/ring.rs).
function measurementRing(rate, channels, pid, writtenNs) {
  const h = Buffer.alloc(256);
  h.write('GLASSTAP', 0, 'latin1');
  h.writeUInt32LE(2, 8);
  h.writeUInt32LE(256, 12);
  h.writeUInt32LE(rate, 16);
  h.writeUInt32LE(channels, 20);
  h.writeUInt32LE(pid, 40);
  h.writeBigUInt64LE(7n, 48);
  h.writeBigUInt64LE(writtenNs, 56);
  return h;
}

// An audio ring's header (crates/tap/src/audio.rs).
function audioRing(rate, channels, format, frameBytes, pid, head, writtenNs, dropped) {
  const h = Buffer.alloc(256);
  h.write('EVOAUDIO', 0, 'latin1');
  h.writeUInt32LE(1, 8);
  h.writeUInt32LE(256, 12);
  h.writeUInt32LE(rate, 16);
  h.writeUInt32LE(channels, 20);
  h.writeUInt32LE(format, 24);
  h.writeUInt32LE(frameBytes, 28);
  h.writeBigUInt64LE(1n << 20n, 32);
  h.writeUInt32LE(pid, 40);
  h.writeUInt32LE(format >= 48 ? 1 : 0, 44);
  h.writeBigUInt64LE(head, 48);
  h.writeBigUInt64LE(writtenNs, 56);
  h.writeBigUInt64LE(dropped, 64);
  return h;
}

test('the sheet reads the measurement rings and the audio rings alike, the audio ring with its format, head and drops', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'glass-rings-'));
  const now = process.hrtime.bigint();
  fs.writeFileSync(path.join(dir, 'glasstap.glass.4242.0'), measurementRing(48000, 2, 4242, now - 100000000n));
  fs.writeFileSync(path.join(dir, 'evoaudio.pub.4242.1'), audioRing(96000, 2, 10, 8, 4242, 123456n, now - 50000000n, 0n));
  fs.writeFileSync(path.join(dir, 'evoaudio.dsd.4243.0'), audioRing(88200, 2, 50, 8, 4243, 99n, now - 9000000000n, 64n));
  fs.writeFileSync(path.join(dir, 'glasstap.demand'), '{}');
  fs.writeFileSync(path.join(dir, 'other.file'), 'x');
  const rings = await readRings(dir);
  assert.deepStrictEqual(rings.map((r) => r.name), ['evoaudio.pub.4242.1', 'glasstap.glass.4242.0', 'evoaudio.dsd.4243.0'], 'the most recently written first, the demand file and strangers left out');
  const audio = rings[0];
  assert.strictEqual(audio.audio, true);
  assert.strictEqual(audio.tag, 'pub');
  assert.strictEqual(audio.format, 'S32_LE');
  assert.strictEqual(audio.frameBytes, 8);
  assert.strictEqual(audio.capacity, 1 << 20);
  assert.strictEqual(audio.seq, 123456, 'the head stands where the sequence does');
  assert.strictEqual(audio.rate, 96000);
  assert.strictEqual(audio.live, true);
  const meter = rings[1];
  assert.strictEqual(meter.audio, false);
  assert.strictEqual(meter.format, undefined);
  assert.strictEqual(meter.seq, 7);
  const dsd = rings[2];
  assert.strictEqual(dsd.format, 'DSD_U32_LE');
  assert.strictEqual(dsd.dropped, 64);
  assert.strictEqual(dsd.live, false, 'written nine seconds ago');
  assert.ok(dsd.writtenAgoMs >= 8900);
  assert.deepStrictEqual(await readRings(path.join(dir, 'missing')), []);
  fs.rmSync(dir, { recursive: true, force: true });
});
