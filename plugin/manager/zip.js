'use strict';
// A reader for the zip files themes come in, and the rule that turns one
// into the folders the display reads. No external module: the central
// directory is parsed here and entries are inflated with zlib.
//
// A theme zip holds one or more units: a directory with meters.txt (a
// meter theme, installed under templates/) or spectrum.txt (a spectrum
// theme, installed under templates_spectrum/). Zips come in several
// layouts, one folder, files at the root, a templates/ prefix, or a bundle
// of folders, and the units name each one, so nothing is guessed at
// install time. This is the same rule the catalog index is generated with.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const zlib = require('zlib');
const { promisify } = require('util');

const inflateRaw = promisify(zlib.inflateRaw);

const SIG_LOCAL = 0x04034b50;
const SIG_CENTRAL = 0x02014b50;
const SIG_EOCD = 0x06054b50;
const SIG_EOCD64_LOCATOR = 0x07064b50;
const SIG_EOCD64 = 0x06064b50;
const METHOD_STORED = 0;
const METHOD_DEFLATE = 8;
const MAX_ENTRIES = 20000;
const MAX_ENTRY_BYTES = 256 * 1024 * 1024;
const MAX_UNPACKED_BYTES = 1024 * 1024 * 1024;
const S_IFMT = 0o170000;
const S_IFREG = 0o100000;

const UNIT_FILES = {
  'meters.txt': { kind: 'meter', install: 'templates' },
  'spectrum.txt': { kind: 'spectrum', install: 'templates_spectrum' }
};
const CONTAINERS = ['', 'templates', 'templates_spectrum'];
const FOLDER_NAME = /^[A-Za-z0-9][A-Za-z0-9 ._()+-]*$/;

const CRC_TABLE = (function () {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
    }
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let crc = -1;
  for (let i = 0; i < buffer.length; i++) {
    crc = CRC_TABLE[(crc ^ buffer[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ -1) >>> 0;
}

class ZipError extends Error {
  constructor(code, message) {
    super(message || code);
    this.code = code;
  }
}

// Whether a name inside a zip may be written under a destination: relative,
// no empty or dot-dot segments, no backslashes or control characters.
function safeName(name) {
  if (typeof name !== 'string' || name.length === 0 || name.length > 1024) return false;
  if (name.startsWith('/') || name.includes('\\') || name.includes('\0')) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f]/.test(name)) return false;
  const parts = name.split('/');
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    const last = i === parts.length - 1;
    if (part === '..' || part === '.') return false;
    if (part === '' && !(last && parts.length > 1)) return false;
  }
  return true;
}

function safeFolderName(folder) {
  return typeof folder === 'string' && folder.length <= 128 && FOLDER_NAME.test(folder) && !folder.includes('..');
}

async function readAt(fd, position, length) {
  const buffer = Buffer.alloc(length);
  let done = 0;
  while (done < length) {
    const { bytesRead } = await fd.read(buffer, done, length - done, position + done);
    if (bytesRead === 0) throw new ZipError('truncated', 'the zip ends early');
    done += bytesRead;
  }
  return buffer;
}

// The extra field record with a given id, or null.
function extraField(extra, id) {
  let at = 0;
  while (at + 4 <= extra.length) {
    const fieldId = extra.readUInt16LE(at);
    const size = extra.readUInt16LE(at + 2);
    if (fieldId === id) return extra.subarray(at + 4, at + 4 + size);
    at += 4 + size;
  }
  return null;
}

class Zip {
  constructor(fd, size, entries) {
    this.fd = fd;
    this.size = size;
    this.entries = entries;
  }

  // Open a zip and read its central directory.
  static async open(file) {
    const fd = await fsp.open(file, 'r');
    try {
      const size = (await fd.stat()).size;
      const entries = await readCentralDirectory(fd, size);
      return new Zip(fd, size, entries);
    } catch (e) {
      await fd.close();
      throw e;
    }
  }

  async close() {
    if (this.fd) {
      await this.fd.close();
      this.fd = null;
    }
  }

  // The bytes of one entry, inflated and checked against its CRC.
  async read(entry) {
    if (entry.size > MAX_ENTRY_BYTES) throw new ZipError('too-large', entry.name + ' is larger than allowed');
    const local = await readAt(this.fd, entry.offset, 30);
    if (local.readUInt32LE(0) !== SIG_LOCAL) throw new ZipError('corrupt', 'local header missing for ' + entry.name);
    const nameLength = local.readUInt16LE(26);
    const extraLength = local.readUInt16LE(28);
    const start = entry.offset + 30 + nameLength + extraLength;
    if (start + entry.compressedSize > this.size) throw new ZipError('truncated', entry.name + ' runs past the end');
    const packed = await readAt(this.fd, start, entry.compressedSize);
    let data;
    if (entry.method === METHOD_STORED) {
      data = packed;
    } else if (entry.method === METHOD_DEFLATE) {
      data = await inflateRaw(packed, { maxOutputLength: MAX_ENTRY_BYTES });
    } else {
      throw new ZipError('unsupported', entry.name + ' uses compression method ' + entry.method);
    }
    if (data.length !== entry.size) throw new ZipError('corrupt', entry.name + ' inflates to the wrong size');
    if (crc32(data) !== entry.crc) throw new ZipError('corrupt', entry.name + ' fails its checksum');
    return data;
  }
}

async function readCentralDirectory(fd, size) {
  if (size < 22) throw new ZipError('not-a-zip', 'the file is too small to be a zip');
  const tailLength = Math.min(size, 22 + 65535);
  const tail = await readAt(fd, size - tailLength, tailLength);
  let eocdAt = -1;
  for (let at = tail.length - 22; at >= 0; at--) {
    if (tail.readUInt32LE(at) === SIG_EOCD) {
      eocdAt = at;
      break;
    }
  }
  if (eocdAt < 0) throw new ZipError('not-a-zip', 'no end of central directory');
  let count = tail.readUInt16LE(eocdAt + 10);
  let directorySize = tail.readUInt32LE(eocdAt + 12);
  let directoryOffset = tail.readUInt32LE(eocdAt + 16);
  if (count === 0xffff || directorySize === 0xffffffff || directoryOffset === 0xffffffff) {
    const locatorAt = eocdAt - 20;
    if (locatorAt < 0 || tail.readUInt32LE(locatorAt) !== SIG_EOCD64_LOCATOR) {
      throw new ZipError('corrupt', 'zip64 locator missing');
    }
    const eocd64Offset = Number(tail.readBigUInt64LE(locatorAt + 8));
    const eocd64 = await readAt(fd, eocd64Offset, 56);
    if (eocd64.readUInt32LE(0) !== SIG_EOCD64) throw new ZipError('corrupt', 'zip64 end record missing');
    count = Number(eocd64.readBigUInt64LE(32));
    directorySize = Number(eocd64.readBigUInt64LE(40));
    directoryOffset = Number(eocd64.readBigUInt64LE(48));
  }
  if (count > MAX_ENTRIES) throw new ZipError('too-many', 'the zip holds more than ' + MAX_ENTRIES + ' entries');
  if (directoryOffset + directorySize > size) throw new ZipError('corrupt', 'the central directory runs past the end');
  const directory = await readAt(fd, directoryOffset, directorySize);
  const entries = [];
  let at = 0;
  for (let i = 0; i < count; i++) {
    if (at + 46 > directory.length || directory.readUInt32LE(at) !== SIG_CENTRAL) {
      throw new ZipError('corrupt', 'central directory entry ' + i + ' is damaged');
    }
    const flags = directory.readUInt16LE(at + 8);
    const method = directory.readUInt16LE(at + 10);
    const crc = directory.readUInt32LE(at + 16);
    let compressedSize = directory.readUInt32LE(at + 20);
    let uncompressedSize = directory.readUInt32LE(at + 24);
    const nameLength = directory.readUInt16LE(at + 28);
    const extraLength = directory.readUInt16LE(at + 30);
    const commentLength = directory.readUInt16LE(at + 32);
    const externalAttributes = directory.readUInt32LE(at + 38);
    let offset = directory.readUInt32LE(at + 42);
    const nameBytes = directory.subarray(at + 46, at + 46 + nameLength);
    const extra = directory.subarray(at + 46 + nameLength, at + 46 + nameLength + extraLength);
    const name = nameBytes.toString('utf8');
    if (compressedSize === 0xffffffff || uncompressedSize === 0xffffffff || offset === 0xffffffff) {
      const zip64 = extraField(extra, 0x0001);
      if (!zip64) throw new ZipError('corrupt', name + ' needs a zip64 record it lacks');
      let zat = 0;
      if (uncompressedSize === 0xffffffff) { uncompressedSize = Number(zip64.readBigUInt64LE(zat)); zat += 8; }
      if (compressedSize === 0xffffffff) { compressedSize = Number(zip64.readBigUInt64LE(zat)); zat += 8; }
      if (offset === 0xffffffff) { offset = Number(zip64.readBigUInt64LE(zat)); zat += 8; }
    }
    if (flags & 0x0001) throw new ZipError('encrypted', name + ' is encrypted');
    const mode = externalAttributes >>> 16;
    const isDirectory = name.endsWith('/');
    // A unix mode says whether the entry is a plain file; a zip made on
    // another system leaves the mode at zero, which is taken as plain.
    const isRegular = !isDirectory && (mode === 0 || (mode & S_IFMT) === S_IFREG);
    entries.push({
      name: name,
      isDirectory: isDirectory,
      isRegular: isRegular,
      method: method,
      crc: crc,
      compressedSize: compressedSize,
      size: uncompressedSize,
      offset: offset
    });
    at += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

// The sections of a PeppyMeter or PeppySpectrum configuration, without the
// [current] bookmark.
function sections(text) {
  const names = [];
  String(text).split(/\r?\n/).forEach(function (line) {
    line = line.trim();
    if (line.startsWith('[') && line.endsWith(']')) {
      const name = line.slice(1, -1).trim();
      if (name.toLowerCase() !== 'current') names.push(name);
    }
  });
  return names;
}

// The installable units of a theme zip and any reason it cannot be
// installed as it is. `defaultFolder` names a unit whose directory is
// the zip root or a plain templates/ container.
async function unitsOf(zip, defaultFolder) {
  const units = [];
  const problems = [];
  const seen = new Set();
  for (const entry of zip.entries) {
    if (entry.name.startsWith('__MACOSX/')) continue;
    if (!safeName(entry.name)) {
      problems.push('unsafe entry ' + JSON.stringify(entry.name));
      continue;
    }
    const slash = entry.name.lastIndexOf('/');
    const head = slash >= 0 ? entry.name.slice(0, slash) : '';
    const base = entry.name.slice(slash + 1);
    const unitFile = UNIT_FILES[base];
    if (!unitFile) continue;
    const leaf = head.slice(head.lastIndexOf('/') + 1);
    const folder = CONTAINERS.includes(leaf) ? defaultFolder : leaf;
    if (!safeFolderName(folder)) {
      problems.push('folder name ' + JSON.stringify(folder));
      continue;
    }
    const key = unitFile.install + '/' + folder;
    if (seen.has(key)) {
      problems.push('two units install to ' + key);
      continue;
    }
    seen.add(key);
    let names = [];
    try {
      names = sections((await zip.read(entry)).toString('utf8'));
    } catch (e) {
      problems.push(entry.name + ': ' + e.message);
    }
    units.push({
      kind: unitFile.kind,
      install: unitFile.install,
      from: head ? head + '/' : '',
      folder: folder,
      names: names
    });
  }
  if (units.length === 0) problems.push('no meters.txt or spectrum.txt');
  return { units: units, problems: problems };
}

// The files of one unit: every regular entry under its prefix, with the
// path it takes under the unit's folder.
function filesOfUnit(zip, unit) {
  const files = [];
  for (const entry of zip.entries) {
    if (!entry.isRegular || entry.name.startsWith('__MACOSX/')) continue;
    if (!entry.name.startsWith(unit.from)) continue;
    const relative = entry.name.slice(unit.from.length);
    if (!relative || !safeName(relative) || relative.endsWith('/')) continue;
    if (path.basename(relative) === '.DS_Store') continue;
    files.push({ entry: entry, relative: relative });
  }
  return files;
}

// Write one unit into `<root>/<folder>/`. The files go to a staging
// directory beside it first and replace the folder in one rename, so a
// failure part way leaves the installed theme as it was.
async function extractUnit(zip, unit, root, options) {
  options = options || {};
  if (!safeFolderName(unit.folder)) throw new ZipError('unsafe', 'folder name ' + JSON.stringify(unit.folder));
  const files = filesOfUnit(zip, unit);
  if (files.length === 0) throw new ZipError('empty', 'the unit ' + unit.folder + ' holds no files');
  const total = files.reduce(function (sum, f) { return sum + f.entry.size; }, 0);
  if (total > MAX_UNPACKED_BYTES) throw new ZipError('too-large', 'the unit ' + unit.folder + ' unpacks to more than allowed');
  const target = path.join(root, unit.folder);
  const staging = path.join(root, '.installing-' + unit.folder + '-' + process.pid);
  const retired = path.join(root, '.replaced-' + unit.folder + '-' + process.pid);
  await fsp.rm(staging, { recursive: true, force: true });
  await fsp.mkdir(staging, { recursive: true });
  let written = 0;
  try {
    for (const file of files) {
      const data = await zip.read(file.entry);
      const destination = path.join(staging, file.relative);
      if (!destination.startsWith(staging + path.sep)) throw new ZipError('unsafe', 'entry escapes its folder: ' + file.relative);
      await fsp.mkdir(path.dirname(destination), { recursive: true });
      await fsp.writeFile(destination, data, { mode: 0o644 });
      written += data.length;
      if (options.onProgress) options.onProgress(written, total);
    }
    let existed = false;
    try {
      await fsp.access(target);
      existed = true;
    } catch (e) { /* new folder */ }
    if (existed) {
      await fsp.rm(retired, { recursive: true, force: true });
      await fsp.rename(target, retired);
    }
    await fsp.rename(staging, target);
    if (existed) await fsp.rm(retired, { recursive: true, force: true });
  } catch (e) {
    await fsp.rm(staging, { recursive: true, force: true });
    try {
      await fsp.access(retired);
      await fsp.rm(target, { recursive: true, force: true });
      await fsp.rename(retired, target);
    } catch (e2) { /* nothing to put back */ }
    throw e;
  }
  return { folder: unit.folder, files: files.length, bytes: written };
}

// The unit's folder as it lands in its tree, for a plan before extraction.
function plan(units, roots) {
  return units.map(function (unit) {
    return {
      kind: unit.kind,
      install: unit.install,
      folder: unit.folder,
      names: unit.names,
      path: path.join(roots[unit.install] || '', unit.folder)
    };
  });
}

module.exports = {
  Zip: Zip,
  ZipError: ZipError,
  crc32: crc32,
  safeName: safeName,
  safeFolderName: safeFolderName,
  sections: sections,
  unitsOf: unitsOf,
  filesOfUnit: filesOfUnit,
  extractUnit: extractUnit,
  plan: plan,
  MAX_UNPACKED_BYTES: MAX_UNPACKED_BYTES
};
