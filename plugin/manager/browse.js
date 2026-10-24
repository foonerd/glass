'use strict';
// The player's browse answers made one shape for the displays and the
// pages: the sources, a location's lists and a search, each as items with
// what a face draws (a title, a line under it, a cover's location or an
// icon's name), what they are and whether they play by themselves. The
// icons are Lucide's names; the faces draw them from their own data.

const plaintext = require('./plaintext');

// Lucide's names for the icon classes the player's lists carry.
const FA_ICONS = {
  'folder-open-o': 'folder', 'folder-o': 'folder', folder: 'folder', 'folder-open': 'folder',
  usb: 'usb', microchip: 'cpu', server: 'server', 'hdd-o': 'hard-drive',
  heart: 'heart', heartbeat: 'heart-pulse', star: 'star', tags: 'tag', tag: 'tag',
  'map-marker': 'map-pin', globe: 'globe', 'thumbs-o-up': 'thumbs-up', 'thumbs-up': 'thumbs-up',
  microphone: 'mic', music: 'music', 'list-ol': 'list-ordered', list: 'list',
  users: 'user-round', user: 'user-round', search: 'search', cloud: 'cloud',
  podcast: 'radio', headphones: 'music', play: 'play'
};
// Lucide's names for the stock pictures the player's own sources wear.
const STOCK_ICONS = {
  favouritesicon: 'heart', playlisticon: 'list', musiclibraryicon: 'music',
  artisticon: 'user-round', albumicon: 'disc-3', genreicon: 'tag',
  dlnaicon: 'cast', webradioicon: 'radio', radioicon: 'radio'
};
// Lucide's names by what the player says an item is.
const KIND_ICONS = {
  folder: 'folder', 'internal-folder': 'cpu', remdisk: 'usb', song: 'music',
  webradio: 'radio', mywebradio: 'radio', 'radio-category': 'radio',
  'radio-favourites': 'heart', 'mywebradio-category': 'heart-pulse',
  playlist: 'list', 'play-playlist': 'list', album: 'disc-3', artist: 'user-round',
  genre: 'tag', cuefile: 'list-ordered', cuesong: 'music', 'streaming-category': 'cloud',
  item: 'music', title: ''
};
// What plays by itself; the rest opens as a folder.
const LEAVES = new Set(['song', 'webradio', 'mywebradio', 'cuesong', 'track', 'item', 'audio']);
// The most items one answer carries.
const MOST = 2000;

function text(v) {
  if (typeof v === 'string') return v.trim();
  if (v === undefined || v === null) return '';
  return String(v);
}

// A stock picture of the player's own sources, as an icon's name; empty
// for a picture that is not one (a service's own logo, a cover).
function stockIcon(albumart) {
  const m = /sourceicon=music_service\/mpd\/([a-z]+)\.png/.exec(text(albumart));
  return m ? (STOCK_ICONS[m[1]] || 'music') : '';
}

function faIcon(cls) {
  const m = /fa-([a-z0-9-]+)/.exec(text(cls));
  return m ? (FA_ICONS[m[1]] || '') : '';
}

// The icon an item wears where it has no cover of its own.
function iconOf(item) {
  return faIcon(item.icon) || stockIcon(item.albumart) || KIND_ICONS[text(item.type)] || 'folder';
}

// The cover's location as the player reports it: a path of the player's
// own or a web address; empty for a stock picture or nothing.
function artOf(item) {
  const art = text(item.albumart);
  if (!art || stockIcon(art)) return '';
  if (art.startsWith('/albumart') || /^https?:\/\//.test(art)) return art;
  return '';
}

function subtitleOf(item) {
  const artist = plaintext.plain(item.artist);
  const album = plaintext.plain(item.album);
  if (artist && album) return artist + ' · ' + album;
  return artist || album;
}

// A name for an item that carries none: its album or its artist, or the
// last part of its location.
function fallbackTitle(item) {
  const uri = text(item.uri);
  const last = uri.split('/').filter((part) => part).pop() || uri;
  return plaintext.plain(item.album) || plaintext.plain(item.artist) || decodeURIComponent(last);
}

// One item as a face draws it; `cover` is the list's own cover, worn by
// a track that has none of its own (an album's tracks).
function itemOf(item, cover) {
  const type = text(item.type);
  const heading = type === 'title';
  const duration = Number(item.duration);
  const own = heading ? '' : artOf(item);
  // The name made plain: a source's markup gone, a badge before the name
  // kept as the icon where the item would wear a folder's.
  const named = plaintext.name(text(item.title) || text(item.name), item.service);
  const icon = heading ? '' : iconOf(item);
  const badged = plaintext.badgeIcon(named.badge);
  return {
    title: named.text || fallbackTitle(item),
    subtitle: heading ? '' : subtitleOf(item),
    art: own || (LEAVES.has(type) ? text(cover) : ''),
    icon: badged && icon === 'folder' ? badged : icon,
    uri: text(item.uri),
    service: text(item.service),
    type: type,
    leaf: LEAVES.has(type),
    heading: heading,
    duration: Number.isFinite(duration) && duration > 0 ? duration : 0
  };
}

// A source's icon by what it is, where it wears no picture of its own.
function sourceIcon(source) {
  const uri = text(source.uri);
  const name = text(source.name).toLowerCase();
  if (uri === 'favourites' || name.includes('favourite') || name.includes('favorite')) return 'heart';
  if (uri === 'playlists' || name.includes('playlist')) return 'list';
  if (uri === 'music-library' || name.includes('library')) return 'music';
  if (uri.startsWith('artists')) return 'user-round';
  if (uri.startsWith('albums')) return 'disc-3';
  if (uri.startsWith('genres')) return 'tag';
  if (uri.startsWith('radio') || name.includes('radio')) return 'radio';
  if (uri.startsWith('upnp') || name.includes('media server')) return 'cast';
  if (uri.startsWith('search')) return 'search';
  return 'cloud';
}

// The sources as the player shows them on its Browse page: the answer to
// a browse of nothing.
function sources(list, serial) {
  const items = (Array.isArray(list) ? list : [])
    .filter((s) => s && typeof s === 'object' && s.name !== undefined)
    .map((s) => ({
      title: plaintext.plain(s.name),
      subtitle: '',
      art: artOf(s),
      icon: faIcon(s.icon) || stockIcon(s.albumart) || sourceIcon(s),
      uri: text(s.uri),
      service: text(s.plugin_name),
      type: 'source',
      leaf: false,
      heading: false,
      duration: 0
    }));
  return { kind: 'browse', uri: '', title: 'Browse', prev: '', info: null, items: items.slice(0, MOST), serial: serial | 0 };
}

// A location's lists as the player answers a browse: one list of items,
// with a heading between the player's lists where it has more than one
// (a search's songs, albums and artists). `title` is the face's word for
// the page (the item it tapped), the album's own where there is one.
function answer(uri, navigation, serial, title) {
  const nav = navigation && typeof navigation === 'object' ? navigation : {};
  const lists = Array.isArray(nav.lists) ? nav.lists : [];
  const items = [];
  const info = nav.info && typeof nav.info === 'object' ? itemOf(nav.info) : null;
  const cover = info ? info.art : '';
  lists.forEach((list) => {
    if (!list || typeof list !== 'object') return;
    const entries = Array.isArray(list.items) ? list.items : [];
    const heading = plaintext.plain(list.title);
    if (lists.length > 1 && heading) {
      items.push({ title: heading, subtitle: '', art: '', icon: '', uri: '', service: '', type: 'title', leaf: false, heading: true, duration: 0 });
    }
    entries.forEach((item) => { if (item && typeof item === 'object') items.push(itemOf(item, cover)); });
  });
  const name = plaintext.plain(title) || (info ? (info.title || plaintext.plain(nav.info.album)) : '');
  return { kind: 'browse', uri: text(uri), title: name, prev: text(nav.prev && nav.prev.uri), info: info, items: items.slice(0, MOST), serial: serial | 0 };
}

// What a tap plays from a list: the items that play by themselves, as the
// player takes them, and the place of the tapped one among them (-1 when
// the tapped one is no such item).
function playable(items, index) {
  const list = [];
  let at = -1;
  (Array.isArray(items) ? items : []).forEach((item, i) => {
    if (!item || !item.leaf) return;
    if (i === index) at = list.length;
    list.push(played(item));
  });
  return { list: list, index: at };
}

// An item as the player's queue takes it.
function played(item) {
  const out = { uri: text(item.uri), service: text(item.service), title: text(item.title), type: text(item.type) };
  const art = text(item.art || item.albumart);
  if (art) out.albumart = art;
  return out;
}

// The last answers by location, so a tap's list is the one the face saw.
class Kept {
  constructor(most) {
    this.most = most || 8;
    this.lines = [];
  }

  put(line) {
    if (!line || typeof line.uri !== 'string') return;
    this.lines = this.lines.filter((l) => l.uri !== line.uri);
    this.lines.push(line);
    while (this.lines.length > this.most) this.lines.shift();
  }

  get(uri) {
    return this.lines.find((l) => l.uri === uri) || null;
  }
}

module.exports = { sources, answer, playable, played, itemOf, iconOf, artOf, Kept, MOST };
