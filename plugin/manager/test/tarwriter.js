'use strict';
// A small gzipped tar writer for the tests: enough of the format to build
// the layouts the reader must handle, a GNU long name among them. Not
// used by the plugin itself.

const zlib = require('zlib');

const BLOCK = 512;

function header(name, size, type) {
  const h = Buffer.alloc(BLOCK);
  h.write(name.slice(0, 100), 0, 'utf8');
  h.write('0000644\0', 100);
  h.write('0000000\0', 108);
  h.write('0000000\0', 116);
  h.write(size.toString(8).padStart(11, '0') + '\0', 124);
  h.write('00000000000\0', 136);
  h.write('        ', 148);
  h.write(type, 156);
  h.write('ustar\0', 257);
  h.write('00', 263);
  let sum = 0;
  for (let i = 0; i < BLOCK; i++) sum += h[i];
  h.write(sum.toString(8).padStart(6, '0') + '\0 ', 148);
  return h;
}

function padded(data) {
  const rest = data.length % BLOCK;
  return rest ? Buffer.concat([data, Buffer.alloc(BLOCK - rest)]) : data;
}

// files: [{ name, data (Buffer|string), dir (bool) }]; a name over a
// hundred characters goes in as GNU tar writes it, a long-name entry first.
function buildTarGz(files, options) {
  options = options || {};
  const parts = [];
  files.forEach(function (file) {
    const data = Buffer.isBuffer(file.data) ? file.data : Buffer.from(file.data || '', 'utf8');
    if (file.name.length > 100) {
      const longName = Buffer.from(file.name + '\0', 'utf8');
      parts.push(header('././@LongLink', longName.length, 'L'), padded(longName));
    }
    parts.push(header(file.name, file.dir ? 0 : data.length, file.dir ? '5' : '0'));
    if (!file.dir) parts.push(padded(data));
  });
  if (!options.truncated) parts.push(Buffer.alloc(BLOCK * 2));
  const tar = Buffer.concat(parts);
  return zlib.gzipSync(options.truncated ? tar.subarray(0, tar.length - 100) : tar);
}

module.exports = { buildTarGz };
