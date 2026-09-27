'use strict';

// Uploaded fonts: what a font file is, by its first bytes, and what it may be called.

const MAX_FONT_BYTES = 64 * 1024 * 1024;

// The kind of a font file by its magic, or null for anything else: TrueType
// (0x00010000 or 'true'), OpenType with CFF outlines ('OTTO'). Collections
// ('ttcf') are refused: one face per file.
function fontKind(bytes) {
  if (!bytes || bytes.length < 4) return null;
  const head = bytes.subarray ? bytes.subarray(0, 4) : bytes.slice(0, 4);
  const text = String.fromCharCode(head[0], head[1], head[2], head[3]);
  if (head[0] === 0 && head[1] === 1 && head[2] === 0 && head[3] === 0) return 'ttf';
  if (text === 'true') return 'ttf';
  if (text === 'OTTO') return 'otf';
  return null;
}

// A file name a font may keep: its base name with the extension the bytes
// say, letters, digits and a few marks only, at most 96 characters.
function safeFontName(rawName, kind) {
  const base = String(rawName || '').replace(/\\/g, '/').split('/').pop().replace(/\.(ttf|otf)$/i, '');
  const clean = base.replace(/[^A-Za-z0-9 ._()+-]/g, '_').replace(/^[. ]+/, '').slice(0, 96).trim();
  if (!clean) return null;
  return clean + '.' + kind;
}

module.exports = { MAX_FONT_BYTES, fontKind, safeFontName };
