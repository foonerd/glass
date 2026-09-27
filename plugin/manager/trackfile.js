'use strict';

// A picture in the playing track's folder, for a remote display: the same
// rule the display applies on the player. The track's location is its
// folder under /mnt, after the player's music-library/ or mnt/ prefix; the
// name is a plain file name with a picture extension.

const path = require('path');

const EXTENSIONS = ['.png', '.jpg', '.jpeg', '.gif', '.webp'];

function trackFolder(uri) {
  let s = String(uri || '').trim();
  // A stream has no folder.
  if (!s || s.includes('://')) return null;
  s = s.replace(/^music-library\/?/, '').replace(/^mnt\/?/, '');
  const base = s.startsWith('/') ? '/mnt' + s : '/mnt/' + s;
  const slash = base.lastIndexOf('/');
  if (slash < 0) return null;
  const folder = path.resolve(base.slice(0, slash));
  if (folder !== '/mnt' && !folder.startsWith('/mnt/')) return null;
  return folder;
}

function trackFilePath(uri, name) {
  const folder = trackFolder(uri);
  const file = String(name || '').trim();
  if (!folder || !file || file.includes('/') || file.includes('\\') || file.includes('..')) return null;
  const lower = file.toLowerCase();
  if (!EXTENSIONS.some((ext) => lower.endsWith(ext))) return null;
  const full = path.resolve(folder, file);
  if (path.dirname(full) !== folder) return null;
  return full;
}

module.exports = { trackFolder, trackFilePath, EXTENSIONS };
