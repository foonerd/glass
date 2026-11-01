'use strict';
// The tap's rings under /dev/shm, read from their headers for the status
// sheet: the stream, the sequence, and whether the writer wrote within
// the last seconds (the display's own notion of a live ring). Times are
// the monotonic clock. The measurement rings (`glasstap.`,
// crates/tap/src/ring.rs) and the audio rings (`evoaudio.`, the
// stream's own bytes, written when a tap's snippet asks,
// crates/tap/src/audio.rs) alike; an audio ring says its format, its
// capacity, its head (the bytes written so far, where a measurement
// ring's sequence stands) and the bytes its writer dropped.

const fsp = require('fs').promises;
const path = require('path');

const RING_DIR = '/dev/shm';
const RING_PREFIX = 'glasstap.';
const AUDIO_RING_PREFIX = 'evoaudio.';
const RING_LIVE_NS = 3000000000n;

// ALSA's names for every format it numbers (snd_pcm_format_t, 0 to 52;
// 29 and 30 are not assigned), as crates/tap/src/sample.rs names them: the
// ring carries any of them at libasound's own size, served by Volumio
// today or not.
const ALSA_FORMATS = {
  0: 'S8', 1: 'U8', 2: 'S16_LE', 3: 'S16_BE', 4: 'U16_LE', 5: 'U16_BE', 6: 'S24_LE', 7: 'S24_BE', 8: 'U24_LE', 9: 'U24_BE',
  10: 'S32_LE', 11: 'S32_BE', 12: 'U32_LE', 13: 'U32_BE', 14: 'FLOAT_LE', 15: 'FLOAT_BE', 16: 'FLOAT64_LE', 17: 'FLOAT64_BE',
  18: 'IEC958_SUBFRAME_LE', 19: 'IEC958_SUBFRAME_BE', 20: 'MU_LAW', 21: 'A_LAW', 22: 'IMA_ADPCM', 23: 'MPEG', 24: 'GSM',
  25: 'S20_LE', 26: 'S20_BE', 27: 'U20_LE', 28: 'U20_BE', 31: 'SPECIAL',
  32: 'S24_3LE', 33: 'S24_3BE', 34: 'U24_3LE', 35: 'U24_3BE', 36: 'S20_3LE', 37: 'S20_3BE', 38: 'U20_3LE', 39: 'U20_3BE',
  40: 'S18_3LE', 41: 'S18_3BE', 42: 'U18_3LE', 43: 'U18_3BE', 44: 'G723_24', 45: 'G723_24_1B', 46: 'G723_40', 47: 'G723_40_1B',
  48: 'DSD_U8', 49: 'DSD_U16_LE', 50: 'DSD_U32_LE', 51: 'DSD_U16_BE', 52: 'DSD_U32_BE'
};

// Every ring under `dir` (/dev/shm unless given), the most recently
// written first; a file that is no ring, or going away, is left out.
async function readRings(dir) {
  dir = dir || RING_DIR;
  let names = [];
  try {
    names = (await fsp.readdir(dir)).filter(function (n) { return n.startsWith(RING_PREFIX) || n.startsWith(AUDIO_RING_PREFIX); });
  } catch (e) {
    return [];
  }
  const now = process.hrtime.bigint();
  const rings = [];
  for (const name of names) {
    let fd = null;
    try {
      fd = await fsp.open(path.join(dir, name), 'r');
      const header = Buffer.alloc(96);
      const { bytesRead } = await fd.read(header, 0, 96, 0);
      const magic = bytesRead >= 64 ? header.toString('latin1', 0, 8) : '';
      const audio = magic === 'EVOAUDIO';
      if (!audio && magic !== 'GLASSTAP') continue;
      const written = header.readBigUInt64LE(56);
      // A write stamped a hair ahead of this reading counts as now, as the
      // display's own rule has it.
      const ago = written === 0n ? null : (written > now ? 0n : now - written);
      const parts = name.slice(audio ? AUDIO_RING_PREFIX.length : RING_PREFIX.length).split('.');
      const ring = {
        name: name,
        tag: parts[0],
        audio: audio,
        pid: header.readUInt32LE(40),
        rate: header.readUInt32LE(16),
        channels: header.readUInt32LE(20),
        seq: Number(header.readBigUInt64LE(48)),
        writtenAgoMs: ago === null ? null : Number(ago / 1000000n),
        live: ago !== null && ago >= 0n && ago < RING_LIVE_NS,
        // How many times the writer made the file again after finding it
        // gone (a logout as its user removes that user's shared memory).
        healed: 0
      };
      if (audio) {
        const format = header.readUInt32LE(24);
        ring.format = ALSA_FORMATS[format] || String(format);
        ring.frameBytes = header.readUInt32LE(28);
        ring.capacity = Number(header.readBigUInt64LE(32));
        ring.dropped = bytesRead >= 72 ? Number(header.readBigUInt64LE(64)) : 0;
        ring.healed = bytesRead >= 84 ? header.readUInt32LE(80) : 0;
      } else {
        ring.healed = bytesRead >= 72 ? header.readUInt32LE(68) : 0;
      }
      rings.push(ring);
    } catch (e) { /* a ring going away */ } finally {
      if (fd) await fd.close().catch(function () { });
    }
  }
  rings.sort(function (a, b) { return (a.writtenAgoMs === null ? Infinity : a.writtenAgoMs) - (b.writtenAgoMs === null ? Infinity : b.writtenAgoMs); });
  return rings;
}

module.exports = { readRings, RING_DIR, RING_PREFIX, AUDIO_RING_PREFIX, ALSA_FORMATS };
