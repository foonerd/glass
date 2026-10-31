'use strict';
// The player's state made whole from the player's own sources. Volumio's
// state can arrive with fields empty that the player knows: the track
// type after an automatic change within a queue (the backend builds the
// state from the queue entry, which some plugins leave bare), or a title
// that is the file's name, an artist and an album missing, for a track a
// DLNA controller pushed. Two sources on the player hold the truth: MPD,
// which plays the track and reports its file, tags and format, and the
// UPnP renderer, which holds the metadata the controller sent. Both are
// read once per track, bounded in time, and only the fields the state
// lacks are filled, and only when the source is playing the same track.
// Nothing here writes anything anywhere.

const net = require('net');
const http = require('http');
const dgram = require('dgram');
const os = require('os');
const plaintext = require('./plaintext');

const MPD_PORT = 6600;
const ASK_MS = 700;
// The fields a state is made whole in, and the two sources' words.
const FIELDS = ['trackType', 'title', 'artist', 'album', 'duration', 'samplerate', 'bitdepth', 'albumart'];
// The cover address Volumio sends for a track without one.
const DEFAULT_ART = '/albumart';
// A title that is the file's name: an audio extension at its end.
const FILE_TITLE = /\.(mp3|flac|wav|aac|m4a|ogg|opus|wma|aiff?|dsf|dff|ape|wv|mp4|alac)$/i;
// The seconds MPD's position may differ from the state's for the same track.
const SAME_TRACK_S = 5;

// ---- MPD -------------------------------------------------------------

// MPD's answer lines, `key: value` each, as an object; a key given twice
// keeps the first (MPD's `status` and `currentsong` give each once).
function parseMpd(text) {
  const out = {};
  String(text || '').split('\n').forEach((line) => {
    const i = line.indexOf(': ');
    if (i <= 0) return;
    const key = line.slice(0, i);
    if (!(key in out)) out[key] = line.slice(i + 2);
  });
  return out;
}

// The track type as Volumio derives it from the file MPD reports: the
// text after the last dot, cut at a question mark, none when longer
// than ten characters; the player's own words for a CD and the services
// it names by their address.
function trackTypeOf(file) {
  const f = String(file || '');
  if (!f) return '';
  if (f.indexOf('cdda:///') !== -1) return 'CD Audio';
  if (f.indexOf('qobuz') !== -1) return 'qobuz';
  if (f.indexOf('tidal') !== -1) return 'tidal';
  let t = f.split('.').pop();
  if (t.length > 10) t = '';
  return t.split('?')[0];
}

// The sample rate, bit depth and channels as Volumio words them from
// MPD's `audio` (`44100:16:2`, `dsd64:1:2`, `96000:f:2`).
function audioOf(audio) {
  const parts = String(audio || '').split(':');
  if (parts.length < 2 || !parts[0]) return null;
  const dsd = { dsd64: '2.82 MHz', dsd128: '5.64 MHz', dsd256: '11.28 MHz', dsd512: '22.58 MHz' };
  if (dsd[parts[0]]) return { samplerate: dsd[parts[0]], bitdepth: '1 bit', channels: 2 };
  const rate = Number(parts[0]);
  if (!Number.isFinite(rate) || rate <= 0) return null;
  const bitdepth = parts[1] === 'f' ? '32 bit' : (Number(parts[1]) ? Number(parts[1]) + ' bit' : '');
  const channels = Number(parts[2]) || 0;
  return { samplerate: rate / 1000 + ' kHz', bitdepth: bitdepth, channels: channels };
}

// Ask MPD on this player for its current song and status, one connection,
// bounded by `timeoutMs`; `{ song, status }` parsed, or null when MPD does
// not answer in time or at all.
function askMpd(options) {
  const opts = options || {};
  const timeoutMs = opts.timeoutMs || ASK_MS;
  return new Promise((resolve) => {
    let text = '';
    let done = false;
    const settle = (value) => { if (!done) { done = true; clearTimeout(timer); try { socket.destroy(); } catch (e) { /* closed */ } resolve(value); } };
    const timer = setTimeout(() => settle(null), timeoutMs);
    const socket = net.connect(opts.port || MPD_PORT, opts.host || '127.0.0.1');
    socket.setNoDelay(true);
    socket.on('error', () => settle(null));
    socket.on('connect', () => { socket.write('command_list_ok_begin\ncurrentsong\nstatus\ncommand_list_end\n'); });
    socket.on('data', (chunk) => {
      text += chunk.toString('utf8');
      // The greeting, two answers each ending in list_OK, then OK.
      if (/\nOK\n$/.test(text) || /^OK MPD[^\n]*\nOK\n$/.test(text)) {
        const body = text.replace(/^OK MPD[^\n]*\n/, '');
        const parts = body.split('list_OK\n');
        if (parts.length < 3) return settle(null);
        settle({ song: parseMpd(parts[0]), status: parseMpd(parts[1]) });
      }
    });
    socket.on('close', () => settle(null));
  });
}

// ---- The UPnP renderer -----------------------------------------------

const RENDERER_ST = 'urn:schemas-upnp-org:device:MediaRenderer:1';
const AVT = 'urn:schemas-upnp-org:service:AVTransport:1';

// The addresses this machine answers to.
function ownAddresses() {
  const out = new Set(['127.0.0.1', '::1']);
  try {
    const ifaces = os.networkInterfaces();
    Object.keys(ifaces).forEach((name) => (ifaces[name] || []).forEach((a) => { if (a && a.address) out.add(a.address); }));
  } catch (e) { /* none known */ }
  return out;
}

// The renderers that answered a search, by their description address; the
// one on this machine is the player's own (`pick`).
function pickOwn(locations, addresses) {
  const own = addresses || ownAddresses();
  return (locations || []).find((l) => {
    const m = /^https?:\/\/\[?([^/\]:]+)\]?(?::\d+)?\//.exec(String(l));
    return m && own.has(m[1]);
  }) || null;
}

// One SSDP search for media renderers, the description addresses that
// answered within `timeoutMs`.
function searchRenderers(timeoutMs) {
  return new Promise((resolve) => {
    const found = [];
    let socket;
    const finish = () => { try { socket.close(); } catch (e) { /* closed */ } resolve(found); };
    try {
      socket = dgram.createSocket('udp4');
    } catch (e) {
      return resolve(found);
    }
    socket.on('error', finish);
    socket.on('message', (msg) => {
      const m = /^LOCATION:\s*(\S+)/im.exec(msg.toString('utf8'));
      if (m && found.indexOf(m[1]) === -1) found.push(m[1]);
    });
    socket.bind(() => {
      try {
        socket.setBroadcast(true);
        const search = 'M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 1\r\nST: ' + RENDERER_ST + '\r\n\r\n';
        socket.send(search, 1900, '239.255.255.250', () => {});
      } catch (e) { /* no network */ }
    });
    setTimeout(finish, timeoutMs || 1500);
  });
}

// A small GET, bounded; the body as text or null.
function fetchText(url, timeoutMs) {
  return new Promise((resolve) => {
    let done = false;
    const settle = (v) => { if (!done) { done = true; resolve(v); } };
    const req = http.get(url, { timeout: timeoutMs || ASK_MS }, (res) => {
      if (res.statusCode !== 200) { res.resume(); return settle(null); }
      let text = '';
      res.setEncoding('utf8');
      res.on('data', (c) => { text += c; if (text.length > 262144) { req.destroy(); settle(null); } });
      res.on('end', () => settle(text));
    });
    req.on('timeout', () => { req.destroy(); settle(null); });
    req.on('error', () => settle(null));
  });
}

// The AVTransport control address in a renderer's description, joined to
// the description's own origin; null without one.
function controlOf(description, location) {
  const m = new RegExp('<serviceType>\\s*' + AVT.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '\\s*</serviceType>[\\s\\S]*?<controlURL>\\s*([^<]+?)\\s*</controlURL>').exec(String(description || ''));
  if (!m) return null;
  const path = m[1];
  if (/^https?:\/\//.test(path)) return path;
  const origin = /^(https?:\/\/[^/]+)/.exec(String(location || ''));
  if (!origin) return null;
  return origin[1] + (path.startsWith('/') ? '' : '/') + path;
}

// Find the player's own renderer: its AVTransport control address, or
// null when none is on this machine.
async function discoverRenderer(options) {
  const opts = options || {};
  const locations = await (opts.search || searchRenderers)(opts.searchMs || 1500);
  const own = pickOwn(locations, opts.addresses);
  if (!own) return null;
  const description = await (opts.fetch || fetchText)(own, opts.timeoutMs || ASK_MS);
  return description ? controlOf(description, own) : null;
}

// One SOAP action with no arguments but the instance, bounded; the
// response body as text or null.
function soap(control, action, timeoutMs) {
  return new Promise((resolve) => {
    let done = false;
    const settle = (v) => { if (!done) { done = true; resolve(v); } };
    let url;
    try { url = new URL(control); } catch (e) { return settle(null); }
    const body = '<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:' + action + ' xmlns:u="' + AVT + '"><InstanceID>0</InstanceID></u:' + action + '></s:Body></s:Envelope>';
    const req = http.request({ hostname: url.hostname, port: url.port || 80, path: url.pathname + url.search, method: 'POST', timeout: timeoutMs || ASK_MS,
      headers: { 'Content-Type': 'text/xml; charset="utf-8"', SOAPACTION: '"' + AVT + '#' + action + '"', 'Content-Length': Buffer.byteLength(body) } }, (res) => {
      let text = '';
      res.setEncoding('utf8');
      res.on('data', (c) => { text += c; if (text.length > 262144) { req.destroy(); settle(null); } });
      res.on('end', () => settle(res.statusCode === 200 ? text : null));
    });
    req.on('timeout', () => { req.destroy(); settle(null); });
    req.on('error', () => settle(null));
    req.end(body);
  });
}

// The text of one element of a SOAP or DIDL document, entities decoded;
// '' when absent. `name` may carry a prefix (`dc:title`).
function elementText(xml, name) {
  const m = new RegExp('<' + name + '(?:\\s[^>]*)?>([\\s\\S]*?)</' + name + '>').exec(String(xml || ''));
  return m ? plaintext.decodeEntities(m[1]).trim() : '';
}

// `hh:mm:ss` as seconds, 0 for none.
function secondsOf(text) {
  const m = /^(\d+):(\d\d):(\d\d)/.exec(String(text || '').trim());
  if (!m) return 0;
  return Number(m[1]) * 3600 + Number(m[2]) * 60 + Number(m[3]);
}

// The DIDL-Lite metadata of a track as the controller sent it: title,
// artist, album, the cover's address; empty where the item carries none.
function parseDidl(didl) {
  const item = plaintext.decodeEntities(String(didl || ''));
  return {
    title: elementText(item, 'dc:title'),
    artist: elementText(item, 'upnp:artist') || elementText(item, 'dc:creator'),
    album: elementText(item, 'upnp:album'),
    albumart: elementText(item, 'upnp:albumArtURI')
  };
}

// Ask the renderer what it plays: `{ transport, uri, duration, meta }`, or
// null when it does not answer. `meta` is the track's DIDL metadata.
async function askRenderer(control, options) {
  const opts = options || {};
  const call = opts.soap || soap;
  const [transport, position, media] = await Promise.all([call(control, 'GetTransportInfo', opts.timeoutMs), call(control, 'GetPositionInfo', opts.timeoutMs), call(control, 'GetMediaInfo', opts.timeoutMs)]);
  if (!transport || !position) return null;
  return {
    transport: elementText(transport, 'CurrentTransportState'),
    uri: elementText(position, 'TrackURI') || elementText(media, 'CurrentURI'),
    duration: secondsOf(elementText(position, 'TrackDuration')),
    meta: parseDidl(elementText(position, 'TrackMetaData') || elementText(media, 'CurrentURIMetaData'))
  };
}

// ---- The rule --------------------------------------------------------

function empty(v) {
  return v === undefined || v === null || String(v).trim() === '';
}

// Whether a title is the file's name rather than a name: an audio
// extension at its end, or the last part of the track's location.
function fileTitle(title, uri) {
  const t = String(title || '').trim();
  if (!t) return false;
  if (FILE_TITLE.test(t)) return true;
  const last = String(uri || '').split('?')[0].split('/').filter(Boolean).pop() || '';
  let decoded = last;
  try { decoded = decodeURIComponent(last); } catch (e) { /* as it came */ }
  return !!last && (t === last || t === decoded);
}

// The fields a playing state lacks, or carries a file's name for; [] for
// a state that wants nothing, or is not playing.
function wanted(state) {
  if (!state || state.status !== 'play') return [];
  const out = [];
  if (empty(state.trackType)) out.push('trackType');
  if (empty(state.title) || fileTitle(state.title, state.uri)) out.push('title');
  if (empty(state.artist)) out.push('artist');
  if (empty(state.album)) out.push('album');
  if (!(Number(state.duration) > 0)) out.push('duration');
  if (empty(state.samplerate)) out.push('samplerate');
  if (empty(state.bitdepth)) out.push('bitdepth');
  if (empty(state.albumart) || String(state.albumart).trim() === DEFAULT_ART) out.push('albumart');
  return out;
}

// Whether MPD plays the state's track: playing, and at the state's
// position give or take a few seconds (the state's `seek` is in
// milliseconds); a state without a position is taken on MPD's word.
function mpdAgrees(state, mpd) {
  if (!mpd || !mpd.status || mpd.status.state !== 'play') return false;
  const seek = Number(state && state.seek);
  const elapsed = Number(mpd.status.elapsed);
  if (!Number.isFinite(seek) || !Number.isFinite(elapsed)) return true;
  return Math.abs(elapsed - seek / 1000) <= SAME_TRACK_S;
}

// Whether the renderer plays the same track MPD plays: playing, and the
// same address, or MPD's answer unknown.
function rendererAgrees(renderer, mpd) {
  if (!renderer || renderer.transport !== 'PLAYING') return false;
  const file = mpd && mpd.song && mpd.song.file;
  if (!file || !renderer.uri) return true;
  return renderer.uri === file || plainAddress(renderer.uri) === plainAddress(file);
}

// An address as plain text: entities decoded as many times as they were
// escaped (the renderer escapes what it was given once more), then the
// percent-encoding.
function plainAddress(s) {
  let out = String(s || '');
  for (let i = 0; i < 3 && out.indexOf('&') !== -1; i += 1) {
    const next = plaintext.decodeEntities(out);
    if (next === out) break;
    out = next;
  }
  try { return decodeURIComponent(out); } catch (e) { return out; }
}

// The state made whole, on a copy: each wanted field from MPD where MPD
// agrees, the names and the cover from the renderer where it agrees and
// MPD had none; `filled` names each field and its source.
function fill(state, mpd, renderer) {
  const need = wanted(state);
  const out = Object.assign({}, state);
  const filled = [];
  if (!need.length) return { state: out, filled: filled };
  const take = (field, value, from) => {
    if (need.indexOf(field) === -1 || empty(value)) return;
    if (filled.some((f) => f.field === field)) return;
    out[field] = value;
    filled.push({ field: field, from: from });
  };
  if (mpdAgrees(state, mpd)) {
    const song = mpd.song || {};
    const status = mpd.status || {};
    take('trackType', trackTypeOf(song.file), 'mpd');
    if (!fileTitle(song.Title, song.file)) take('title', song.Title, 'mpd');
    take('artist', song.Artist || song.Name, 'mpd');
    take('album', song.Album, 'mpd');
    const seconds = Number(song.Time) || Number(status.duration) || 0;
    if (seconds > 0) take('duration', Math.round(seconds), 'mpd');
    const audio = audioOf(status.audio);
    if (audio) {
      take('samplerate', audio.samplerate, 'mpd');
      take('bitdepth', audio.bitdepth, 'mpd');
    }
  }
  if (rendererAgrees(renderer, mpd)) {
    const meta = renderer.meta || {};
    if (!fileTitle(meta.title, renderer.uri)) take('title', meta.title, 'renderer');
    take('artist', meta.artist, 'renderer');
    take('album', meta.album, 'renderer');
    if (/^https?:\/\//.test(meta.albumart || '')) take('albumart', meta.albumart, 'renderer');
    if (renderer.duration > 0) take('duration', renderer.duration, 'renderer');
  }
  return { state: out, filled: filled };
}

// ---- The player's own queue ------------------------------------------

// The queue item that is the state's track: the same address, as given
// or as plain text. Null without one.
function queueItemOf(state, queue) {
  const uri = String(state && state.uri || '');
  if (!uri || !Array.isArray(queue)) return null;
  const plain = plainAddress(uri);
  return queue.find((it) => it && typeof it === 'object' && String(it.uri || '') === uri)
    || queue.find((it) => it && typeof it === 'object' && it.uri && plainAddress(it.uri) === plain)
    || null;
}

// The state made whole from the player's own queue, on a copy: the names,
// the cover and the length of the item that is the state's track, where
// the state lacks them. Volumio knows these for a track from a media
// server it browsed, and loses them the moment the track plays, when it
// builds the state from MPD's bare stream address instead; pause and
// play brings them back. No network, nothing asked.
function fillFromQueue(state, queue) {
  const need = wanted(state);
  const out = Object.assign({}, state);
  const filled = [];
  const item = need.length ? queueItemOf(state, queue) : null;
  if (!item) return { state: out, filled: filled };
  const take = (field, value) => {
    if (need.indexOf(field) === -1 || empty(value)) return;
    out[field] = value;
    filled.push({ field: field, from: 'queue' });
  };
  if (!fileTitle(item.title, item.uri)) take('title', item.title);
  take('artist', item.artist);
  take('album', item.album);
  if (/^(https?:\/\/|\/)/.test(String(item.albumart || '')) && String(item.albumart).trim() !== DEFAULT_ART) take('albumart', item.albumart);
  const seconds = Number(item.duration) || 0;
  if (seconds > 0) take('duration', Math.round(seconds));
  return { state: out, filled: filled };
}

// ---- The tally -------------------------------------------------------

// What was filled since the backend started: how many states, which
// fields, from which sources, and the last time; for the sheet and Find a
// problem.
const tally = { count: 0, fields: [], from: [], at: null };

function noted(filled) {
  if (!filled || !filled.length) return;
  tally.count += 1;
  tally.at = new Date().toISOString();
  filled.forEach((f) => {
    if (tally.fields.indexOf(f.field) === -1) tally.fields.push(f.field);
    if (tally.from.indexOf(f.from) === -1) tally.from.push(f.from);
  });
}

function fact() {
  return { count: tally.count, fields: tally.fields.slice(), from: tally.from.slice(), at: tally.at };
}

function resetTally() {
  tally.count = 0; tally.fields = []; tally.from = []; tally.at = null;
}

// ---- The whole ---------------------------------------------------------

// A state made whole from the player's sources where it wants anything:
// MPD always, the renderer where it is known or can be found. Resolves
// with `{ state, filled }`, the state as given where nothing was wanted or
// nothing answered. `cache.control` keeps the renderer's address between
// calls; a renderer that stops answering is looked for again.
async function assure(state, cache, options) {
  const opts = options || {};
  if (!wanted(state).length) return { state: state, filled: [] };
  // The player's own queue first: it costs nothing and knows the names
  // of a track from a media server the player browsed.
  const own = fillFromQueue(state, opts.queue);
  state = own.state;
  const need = wanted(state);
  if (!need.length) { noted(own.filled); return own; }
  const c = cache || {};
  const mpdAsk = (opts.askMpd || askMpd)(opts.mpd);
  let rendererAsk = Promise.resolve(null);
  const namesWanted = need.some((f) => f === 'title' || f === 'artist' || f === 'album' || f === 'albumart');
  if (namesWanted) {
    rendererAsk = (async () => {
      if (!c.control && !c.lookedAt) {
        c.lookedAt = Date.now();
        c.control = await (opts.discover || discoverRenderer)(opts.renderer);
      }
      if (!c.control) return null;
      const answer = await (opts.askRenderer || askRenderer)(c.control, opts.renderer);
      if (!answer) { c.control = null; c.lookedAt = null; }
      return answer;
    })();
  }
  const [mpd, renderer] = await Promise.all([mpdAsk, rendererAsk]);
  const made = fill(state, mpd, renderer);
  made.filled = own.filled.concat(made.filled);
  noted(made.filled);
  return made;
}

module.exports = { assure, fill, fillFromQueue, queueItemOf, wanted, fileTitle, plainAddress, trackTypeOf, audioOf, parseMpd, askMpd, parseDidl, elementText, secondsOf, controlOf, pickOwn, askRenderer, discoverRenderer, searchRenderers, mpdAgrees, rendererAgrees, fact, resetTally, FIELDS, MPD_PORT, ASK_MS };
