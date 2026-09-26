'use strict';
// A zip writer for one purpose: keeping a copy of the installed plugin
// before an upgrade, so it can be put back. Files are stored as they are,
// one at a time, in the layout Volumio's plugin manager unpacks (the
// plugin's files at the zip root). Symbolic links are left out.

const fs = require('fs');
const fsp = require('fs/promises');
const path = require('path');
const { crc32 } = require('./zip');

const MAX_FILES = 60000;

function write(stream, buffer) {
  return new Promise(function (resolve, reject) {
    if (stream.write(buffer)) return resolve();
    stream.once('drain', resolve);
    stream.once('error', reject);
  });
}

async function walk(dir, base, out) {
  const names = (await fsp.readdir(dir)).sort();
  for (const name of names) {
    const full = path.join(dir, name);
    const stat = await fsp.lstat(full);
    if (stat.isSymbolicLink()) continue;
    if (stat.isDirectory()) {
      await walk(full, base, out);
    } else if (stat.isFile()) {
      out.push({ path: full, name: path.relative(base, full).split(path.sep).join('/'), size: stat.size, mode: stat.mode });
      if (out.length > MAX_FILES) throw new Error('more than ' + MAX_FILES + ' files');
    }
  }
}

// Write `dir` into `file` as a zip. Resolves with the file count and bytes.
async function zipDirectory(dir, file, options) {
  options = options || {};
  const files = [];
  await walk(dir, dir, files);
  const total = files.reduce(function (n, f) { return n + f.size; }, 0);
  const out = fs.createWriteStream(file);
  const central = [];
  let offset = 0;
  let done = 0;
  try {
    for (const entry of files) {
      const data = await fsp.readFile(entry.path);
      const crc = crc32(data);
      const name = Buffer.from(entry.name, 'utf8');
      const local = Buffer.alloc(30);
      local.writeUInt32LE(0x04034b50, 0);
      local.writeUInt16LE(20, 4);
      local.writeUInt16LE(0x0800, 6);
      local.writeUInt16LE(0, 8);
      local.writeUInt32LE(crc, 14);
      local.writeUInt32LE(data.length, 18);
      local.writeUInt32LE(data.length, 22);
      local.writeUInt16LE(name.length, 26);
      const header = Buffer.alloc(46);
      header.writeUInt32LE(0x02014b50, 0);
      header.writeUInt16LE(0x031e, 4);
      header.writeUInt16LE(20, 6);
      header.writeUInt16LE(0x0800, 8);
      header.writeUInt16LE(0, 10);
      header.writeUInt32LE(crc, 16);
      header.writeUInt32LE(data.length, 20);
      header.writeUInt32LE(data.length, 24);
      header.writeUInt16LE(name.length, 28);
      header.writeUInt32LE(((entry.mode & 0o177777) << 16) >>> 0, 38);
      header.writeUInt32LE(offset, 42);
      central.push(header, name);
      await write(out, local);
      await write(out, name);
      await write(out, data);
      offset += local.length + name.length + data.length;
      done += data.length;
      if (options.onProgress) options.onProgress(done, total);
    }
    const directory = Buffer.concat(central);
    const eocd = Buffer.alloc(22);
    eocd.writeUInt32LE(0x06054b50, 0);
    eocd.writeUInt16LE(files.length, 8);
    eocd.writeUInt16LE(files.length, 10);
    eocd.writeUInt32LE(directory.length, 12);
    eocd.writeUInt32LE(offset, 16);
    await write(out, directory);
    await write(out, eocd);
    await new Promise(function (resolve, reject) { out.end(function (e) { return e ? reject(e) : resolve(); }); });
  } catch (e) {
    out.destroy();
    await fsp.rm(file, { force: true });
    throw e;
  }
  return { files: files.length, bytes: offset };
}

module.exports = { zipDirectory: zipDirectory };
