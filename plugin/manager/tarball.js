'use strict';
// A gzipped tar read as the Manager's zip reader reads a zip: `entries`
// with name, isRegular and size, and `read(entry)`, so the signature an
// archive carries inside is held the same way for both kinds. The tar is
// unpacked to a file beside the archive first, streamed, so an archive of
// tens of megabytes never sits whole in memory; `close` removes it. GNU
// tar's long names and pax path records are read; links and devices are
// listed and not regular.

const fs = require('fs');
const fsp = require('fs/promises');
const zlib = require('zlib');
const { Transform } = require('stream');
const { pipeline } = require('stream/promises');

const BLOCK = 512;
// The most a tar may unpack to, the remote's own bound.
const MAX_UNPACKED = 256 * 1024 * 1024;
const MAX_ENTRY_BYTES = 64 * 1024 * 1024;

class TarError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

// A text field of a header: up to the first NUL.
function field(buf, at, len) {
  const slice = buf.subarray(at, at + len);
  const nul = slice.indexOf(0);
  return (nul >= 0 ? slice.subarray(0, nul) : slice).toString('utf8');
}

function octal(buf, at, len) {
  const text = field(buf, at, len).trim();
  if (!text) return 0;
  const n = parseInt(text, 8);
  if (!Number.isFinite(n) || n < 0) throw new TarError('corrupt', 'a tar header holds no number where one is due');
  return n;
}

async function readAt(fd, at, len) {
  const buf = Buffer.alloc(len);
  const { bytesRead } = await fd.read(buf, 0, len, at);
  if (bytesRead !== len) throw new TarError('truncated', 'the tar ends before an entry does');
  return buf;
}

// The `path` record of a pax header, or null.
function paxPath(body) {
  let at = 0;
  const text = body.toString('utf8');
  while (at < text.length) {
    const space = text.indexOf(' ', at);
    if (space < 0) break;
    const len = parseInt(text.slice(at, space), 10);
    if (!Number.isFinite(len) || len <= 0) break;
    const record = text.slice(space + 1, at + len - 1);
    const eq = record.indexOf('=');
    if (eq > 0 && record.slice(0, eq) === 'path') return record.slice(eq + 1);
    at += len;
  }
  return null;
}

// Every entry of an unpacked tar: name, size, where its bytes begin, and
// whether it is a regular file.
async function walk(fd, size) {
  if (size < BLOCK) throw new TarError('not-a-tar', 'the file is not a tar');
  const entries = [];
  let at = 0;
  let nextName = null;
  while (at + BLOCK <= size) {
    const h = await readAt(fd, at, BLOCK);
    if (h.every(function (b) { return b === 0; })) break;
    if (!field(h, 257, 6).startsWith('ustar')) throw new TarError('not-a-tar', 'the file is not a tar');
    let sum = 0;
    for (let i = 0; i < BLOCK; i++) sum += (i >= 148 && i < 156) ? 32 : h[i];
    if (sum !== octal(h, 148, 8)) throw new TarError('corrupt', 'a tar header fails its checksum');
    const type = h[156] === 0 ? '0' : String.fromCharCode(h[156]);
    const entrySize = octal(h, 124, 12);
    const dataAt = at + BLOCK;
    const blocks = Math.ceil(entrySize / BLOCK);
    if (dataAt + blocks * BLOCK > size) throw new TarError('truncated', 'the tar ends before an entry does');
    if (type === 'L') {
      nextName = (await readAt(fd, dataAt, entrySize)).toString('utf8').replace(/\0+$/, '');
    } else if (type === 'x') {
      const given = paxPath(await readAt(fd, dataAt, entrySize));
      if (given) nextName = given;
    } else if (type !== 'K' && type !== 'g') {
      let name = field(h, 0, 100);
      const prefix = field(h, 345, 155);
      if (prefix) name = prefix + '/' + name;
      if (nextName !== null) { name = nextName; nextName = null; }
      entries.push({ name: name, size: entrySize, offset: dataAt, isRegular: type === '0' || type === '7' });
    }
    at = dataAt + blocks * BLOCK;
  }
  return entries;
}

class TarGz {
  constructor(fd, tar, entries) {
    this.fd = fd;
    this.tar = tar;
    this.entries = entries;
  }

  // Open a gzipped tar: unpacked beside it, capped, and walked.
  static async open(file) {
    const tar = file + '.tar';
    let total = 0;
    const cap = new Transform({
      transform(chunk, encoding, done) {
        total += chunk.length;
        if (total > MAX_UNPACKED) return done(new TarError('too-large', 'the archive unpacks to more than allowed'));
        done(null, chunk);
      }
    });
    try {
      await pipeline(fs.createReadStream(file), zlib.createGunzip(), cap, fs.createWriteStream(tar));
    } catch (e) {
      await fsp.rm(tar, { force: true });
      throw e instanceof TarError ? e : new TarError('not-a-tar', 'the file is not a gzipped tar: ' + (e && e.message ? e.message : e));
    }
    const fd = await fsp.open(tar, 'r');
    try {
      const entries = await walk(fd, (await fd.stat()).size);
      return new TarGz(fd, tar, entries);
    } catch (e) {
      await fd.close();
      await fsp.rm(tar, { force: true });
      throw e;
    }
  }

  async close() {
    if (this.fd) {
      await this.fd.close();
      this.fd = null;
    }
    await fsp.rm(this.tar, { force: true });
  }

  // The bytes of one entry.
  async read(entry) {
    if (entry.size > MAX_ENTRY_BYTES) throw new TarError('too-large', entry.name + ' is larger than allowed');
    return readAt(this.fd, entry.offset, entry.size);
  }
}

// Whether a file begins as a zip does; the other archives are gzipped tars.
async function isZipFile(file) {
  const fd = await fsp.open(file, 'r');
  try {
    const head = Buffer.alloc(2);
    const { bytesRead } = await fd.read(head, 0, 2, 0);
    return bytesRead === 2 && head[0] === 0x50 && head[1] === 0x4b;
  } finally {
    await fd.close();
  }
}

module.exports = { TarGz, TarError, isZipFile, MAX_UNPACKED };
