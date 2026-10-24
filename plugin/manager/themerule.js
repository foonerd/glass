'use strict';
// The edition tag rules: at the start of playback, a tag picks a theme
// folder. The tag is the last `[...]` in the album name, else in the
// folder that holds the file; where the name carries none, the source the
// music plays from stands in as the tag, by a word of its own or by the
// service id the player reports, so a web radio stream or a YouTube video,
// which have no folder of the listener's making, can pick a theme too. A
// tag written in the name wins over the source. The rules are one line,
// `Word=folder` entries parted by commas or new lines; a word is matched
// without regard to case; a folder with a slash or a `..` is refused.

// The word each source answers to, by the service id the player reports.
// A source not here answers to its id itself.
const SOURCE_WORDS = {
  webradio: 'radio',
  youtube2: 'youtube',
  ytmusic: 'youtubemusic',
  spop: 'spotify',
  bandcamp: 'bandcamp',
  soundcloud: 'soundcloud',
  mixcloud: 'mixcloud',
  jellyfin: 'jellyfin',
  airplay_emulation: 'airplay',
  mpd: 'local'
};

// The last `[tag]` in a name, '' for none; what follows the bracket is ignored.
function lastEditionTag(text) {
  let last = '';
  let match;
  const re = /\[([^[\]]+)\]/g;
  const source = String(text || '');
  while ((match = re.exec(source))) {
    last = match[1].trim();
  }
  return last;
}

// The name of the folder that holds the file a location names, '' where
// the location has no folder above the file.
function folderNameFromUri(uri) {
  let path = String(uri || '').split('?')[0];
  try { path = decodeURIComponent(path); } catch (e) { /* as it came */ }
  const parts = path.split('/').filter((part) => part !== '');
  if (parts.length < 2) return '';
  return parts[parts.length - 2];
}

// The rules as `{ tag, folder }`, the tag lowercase; a line without `=`,
// an empty side, or a folder that leaves the themes' folder is dropped.
function parseThemeTagRules(text) {
  const rules = [];
  String(text || '').split(/\r?\n|,/).forEach((line) => {
    const eq = line.indexOf('=');
    if (eq <= 0) return;
    const tag = line.slice(0, eq).trim().toLowerCase();
    const folder = line.slice(eq + 1).trim();
    if (!tag || !folder) return;
    if (folder.indexOf('/') !== -1 || folder.indexOf('..') !== -1) return;
    rules.push({ tag: tag, folder: folder });
  });
  return rules;
}

// The words a playing source answers to: its own word where it has one,
// and the service id as the player reports it; none for no service.
function sourceTags(service) {
  const id = String(service || '').trim().toLowerCase();
  if (!id) return [];
  const word = SOURCE_WORDS[id];
  return word && word !== id ? [word, id] : [id];
}

// What the rules pick for what plays: `null` with no rules at all; else
// `{ folder, tag, by }`, the folder '' where nothing matched, `by` one of
// `album`, `folder`, `source` or '' (how the tag was found). The album's
// tag is read first, then the folder's, then the source's words.
function pickTheme(album, uri, service, rulesText) {
  const rules = parseThemeTagRules(rulesText);
  if (!rules.length) return null;
  const map = {};
  rules.forEach((rule) => { map[rule.tag] = rule.folder; });
  const albumTag = lastEditionTag(album).toLowerCase();
  if (albumTag && map[albumTag]) return { folder: map[albumTag], tag: albumTag, by: 'album' };
  const folderTag = lastEditionTag(folderNameFromUri(uri)).toLowerCase();
  if (folderTag && map[folderTag]) return { folder: map[folderTag], tag: folderTag, by: 'folder' };
  const words = sourceTags(service);
  for (let i = 0; i < words.length; i += 1) {
    if (map[words[i]]) return { folder: map[words[i]], tag: words[i], by: 'source' };
  }
  return { folder: '', tag: '', by: '' };
}

// The folder alone, as the plugin has long asked: `null` with no rules,
// '' for no match.
function themeFolderForEdition(album, uri, rulesText, service) {
  const picked = pickTheme(album, uri, service, rulesText);
  return picked === null ? null : picked.folder;
}

module.exports = { lastEditionTag, folderNameFromUri, parseThemeTagRules, sourceTags, pickTheme, themeFolderForEdition, SOURCE_WORDS };
