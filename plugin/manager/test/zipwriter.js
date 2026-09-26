'use strict';
// A small zip writer for the tests: enough of the format to build the
// layouts the reader must handle. Not used by the plugin itself.

const zlib = require('zlib');
const { crc32 } = require('../zip');

// files: [{ name, data (Buffer|string), stored (bool), mode (number) }]
function buildZip(files) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  files.forEach(function (file) {
    const data = Buffer.isBuffer(file.data) ? file.data : Buffer.from(file.data || '', 'utf8');
    const isDirectory = file.name.endsWith('/');
    const packed = (file.stored || isDirectory) ? data : zlib.deflateRawSync(data);
    const method = (file.stored || isDirectory) ? 0 : 8;
    const crc = crc32(data);
    const name = Buffer.from(file.name, 'utf8');
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt16LE(method, 8);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(packed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(name.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(0x031e, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt16LE(method, 10);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(packed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(name.length, 28);
    const mode = file.mode !== undefined ? file.mode : (isDirectory ? 0o040755 : 0o100644);
    central.writeUInt32LE((mode << 16) >>> 0, 38);
    central.writeUInt32LE(offset, 42);
    locals.push(local, name, packed);
    centrals.push(central, name);
    offset += local.length + name.length + packed.length;
  });
  const directory = Buffer.concat(centrals);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(files.length, 8);
  eocd.writeUInt16LE(files.length, 10);
  eocd.writeUInt32LE(directory.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat(locals.concat([directory, eocd]));
}

module.exports = { buildZip: buildZip };
