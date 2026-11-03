'use strict';

const test = require('node:test');
const assert = require('node:assert');
const pcm = require('../pcm');

const near = (a, b) => Math.abs(a - b) < 1e-6;

function segment(format, bps, channels) {
  return { rate: 48000, channels, format, bits: bps * 8, frame_bytes: bps * channels, one_bit: false, capped: false, chunk_ms: 20 };
}

test('s16le: two frames of stereo, each sample in -1..1, the channels apart', () => {
  const b = Buffer.alloc(8);
  b.writeInt16LE(16384, 0); b.writeInt16LE(-16384, 2); b.writeInt16LE(32767, 4); b.writeInt16LE(-32768, 6);
  const out = pcm.decode(segment('s16le', 2, 2), b.buffer.slice(b.byteOffset, b.byteOffset + 8));
  assert.strictEqual(out.length, 2);
  assert.strictEqual(out[0].length, 2);
  assert.ok(near(out[0][0], 0.5) && near(out[1][0], -0.5));
  assert.ok(near(out[0][1], 32767 / 32768) && near(out[1][1], -1));
});

test('s24le: three bytes a sample, the sign carried by the top byte', () => {
  const b = Buffer.alloc(6);
  // +4194304 (half scale) and -8388608 (full negative) as little-endian 24-bit.
  b[0] = 0x00; b[1] = 0x00; b[2] = 0x40;
  b[3] = 0x00; b[4] = 0x00; b[5] = 0x80;
  const out = pcm.decode(segment('s24le', 3, 2), b.buffer.slice(b.byteOffset, b.byteOffset + 6));
  assert.ok(near(out[0][0], 0.5), String(out[0][0]));
  assert.ok(near(out[1][0], -1), String(out[1][0]));
  // A small negative: -1 is ff ff ff.
  const c = Buffer.from([0xff, 0xff, 0xff, 0x01, 0x00, 0x00]);
  const o2 = pcm.decode(segment('s24le', 3, 2), c.buffer.slice(c.byteOffset, c.byteOffset + 6));
  assert.ok(near(o2[0][0], -1 / 8388608));
  assert.ok(near(o2[1][0], 1 / 8388608));
});

test('s32le and f32le as their names say', () => {
  const b = Buffer.alloc(8);
  b.writeInt32LE(1073741824, 0); b.writeInt32LE(-2147483648, 4);
  const o = pcm.decode(segment('s32le', 4, 2), b.buffer.slice(b.byteOffset, b.byteOffset + 8));
  assert.ok(near(o[0][0], 0.5) && near(o[1][0], -1));
  const f = Buffer.alloc(8);
  f.writeFloatLE(0.25, 0); f.writeFloatLE(-0.75, 4);
  const of = pcm.decode(segment('f32le', 4, 2), f.buffer.slice(f.byteOffset, f.byteOffset + 8));
  assert.ok(near(of[0][0], 0.25) && near(of[1][0], -0.75));
});

test('a mono segment, a frame cut short, and the count of whole frames', () => {
  const b = Buffer.alloc(7);
  b.writeInt16LE(100, 0); b.writeInt16LE(200, 2); b.writeInt16LE(300, 4);
  const seg = segment('s16le', 2, 1);
  assert.strictEqual(pcm.framesIn(seg, 7), 3, 'the odd byte is not a frame');
  const out = pcm.decode(seg, b.buffer.slice(b.byteOffset, b.byteOffset + 7));
  assert.strictEqual(out.length, 1);
  assert.strictEqual(out[0].length, 3);
  assert.ok(near(out[0][2], 300 / 32768));
});

test('one-bit audio and an unknown format cannot be played here', () => {
  const dsd = Object.assign(segment('s16le', 2, 2), { one_bit: true });
  assert.strictEqual(pcm.playable(dsd), false);
  assert.strictEqual(pcm.decode(dsd, new ArrayBuffer(8)), null);
  const odd = segment('u8', 1, 2);
  assert.strictEqual(pcm.playable(odd), false);
  assert.strictEqual(pcm.decode(odd, new ArrayBuffer(8)), null);
  assert.strictEqual(pcm.playable(null), false);
  assert.strictEqual(pcm.playable(segment('s16le', 2, 0)), false);
});
