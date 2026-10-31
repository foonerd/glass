'use strict';

// The state made whole from the player's sources: Volumio's own rules for
// the track type and the audio format, the two sources' answer shapes as
// read on a player, the guards that keep a wrong track out, the fill's
// precedence, and the exchanges themselves against a fake MPD and a fake
// renderer, with their timeouts.

const test = require('node:test');
const assert = require('node:assert');
const net = require('net');
const http = require('http');
const A = require('../assure');

const MPD_SONG = 'file: https://rr3---sn-x.googlevideo.com/videoplayback?expire=1&t.YouTube\nTitle: Casa Tupka\nArtist: Dai\nTime: 214\nduration: 214.123\nPos: 0\nId: 7\n';
const MPD_STATUS = 'volume: 40\nrepeat: 0\nrandom: 0\nsingle: 0\nconsume: 1\nstate: play\nsong: 0\nsongid: 7\ntime: 12:214\nelapsed: 12.345\nbitrate: 160\nduration: 214.123\naudio: 44100:f:2\n';
const DIDL = '&lt;DIDL-Lite xmlns:dc=&quot;http://purl.org/dc/elements/1.1/&quot; xmlns:upnp=&quot;urn:schemas-upnp-org:metadata-1-0/upnp/&quot; xmlns=&quot;urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/&quot;&gt;&lt;item id=&quot;1&quot; parentID=&quot;0&quot; restricted=&quot;1&quot;&gt;&lt;dc:title&gt;So Far Away&lt;/dc:title&gt;&lt;upnp:artist&gt;Dire Straits&lt;/upnp:artist&gt;&lt;upnp:album&gt;Brothers In Arms&lt;/upnp:album&gt;&lt;upnp:albumArtURI&gt;http://192.0.2.9:32400/library/metadata/1/thumb.jpg&lt;/upnp:albumArtURI&gt;&lt;upnp:class&gt;object.item.audioItem.musicTrack&lt;/upnp:class&gt;&lt;/item&gt;&lt;/DIDL-Lite&gt;';
const soapOf = (action, inner) => '<?xml version="1.0"?>\n<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">\n<s:Body>\n<u:' + action + 'Response xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">\n' + inner + '\n</u:' + action + 'Response>\n</s:Body></s:Envelope>';
const RENDERER = {
  GetTransportInfo: soapOf('GetTransportInfo', '<CurrentTransportState>PLAYING</CurrentTransportState>\n<CurrentTransportStatus>OK</CurrentTransportStatus>\n<CurrentSpeed>1</CurrentSpeed>'),
  GetPositionInfo: soapOf('GetPositionInfo', '<Track>1</Track>\n<TrackDuration>00:03:34</TrackDuration>\n<TrackMetaData>' + DIDL + '</TrackMetaData>\n<TrackURI>http://192.0.2.9:32400/library/parts/1/file.mp3</TrackURI>\n<RelTime>00:00:12</RelTime>'),
  GetMediaInfo: soapOf('GetMediaInfo', '<NrTracks>1</NrTracks>\n<MediaDuration>00:03:34</MediaDuration>\n<CurrentURI>http://192.0.2.9:32400/library/parts/1/file.mp3</CurrentURI>\n<CurrentURIMetaData>' + DIDL + '</CurrentURIMetaData>')
};

test("the track type is derived as Volumio derives it from MPD's file", () => {
  assert.strictEqual(A.trackTypeOf('https://x/videoplayback?expire=1&t.YouTube'), 'YouTube');
  assert.strictEqual(A.trackTypeOf('INTERNAL/a/b/song.flac'), 'flac');
  assert.strictEqual(A.trackTypeOf('http://stream.example/live'), '', 'a stream with no dot after the host beyond its name: the tail is too long');
  assert.strictEqual(A.trackTypeOf('http://stream.example/live.mp3?x=1'), 'mp3');
  assert.strictEqual(A.trackTypeOf('cdda:///3'), 'CD Audio');
  assert.strictEqual(A.trackTypeOf('qobuz://track/1'), 'qobuz');
  assert.strictEqual(A.trackTypeOf('tidal://track/1'), 'tidal');
  assert.strictEqual(A.trackTypeOf(''), '');
});

test("the audio format is worded as Volumio words MPD's", () => {
  assert.deepStrictEqual(A.audioOf('44100:16:2'), { samplerate: '44.1 kHz', bitdepth: '16 bit', channels: 2 });
  assert.deepStrictEqual(A.audioOf('96000:24:2'), { samplerate: '96 kHz', bitdepth: '24 bit', channels: 2 });
  assert.deepStrictEqual(A.audioOf('44100:f:2'), { samplerate: '44.1 kHz', bitdepth: '32 bit', channels: 2 });
  assert.deepStrictEqual(A.audioOf('dsd64:1:2'), { samplerate: '2.82 MHz', bitdepth: '1 bit', channels: 2 });
  assert.strictEqual(A.audioOf(''), null);
  assert.strictEqual(A.audioOf('x:y'), null);
});

test("MPD's lines parse, the first of a key kept", () => {
  const song = A.parseMpd(MPD_SONG);
  assert.strictEqual(song.Title, 'Casa Tupka');
  assert.strictEqual(song.file, 'https://rr3---sn-x.googlevideo.com/videoplayback?expire=1&t.YouTube');
  assert.strictEqual(A.parseMpd('a: 1\na: 2\nnocolon\n').a, '1');
});

test("the renderer's answers parse: the transport, the address, the duration and the controller's metadata, double-escaped", () => {
  assert.strictEqual(A.elementText(RENDERER.GetTransportInfo, 'CurrentTransportState'), 'PLAYING');
  assert.strictEqual(A.secondsOf('00:03:34'), 214);
  assert.strictEqual(A.secondsOf('NOT_IMPLEMENTED'), 0);
  const meta = A.parseDidl(A.elementText(RENDERER.GetPositionInfo, 'TrackMetaData'));
  assert.deepStrictEqual(meta, { title: 'So Far Away', artist: 'Dire Straits', album: 'Brothers In Arms', albumart: 'http://192.0.2.9:32400/library/metadata/1/thumb.jpg' });
  assert.deepStrictEqual(A.parseDidl(''), { title: '', artist: '', album: '', albumart: '' });
  const desc = '<root><device><serviceList><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/u/ctl-rc</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/u/ctl-avt</controlURL></service></serviceList></device></root>';
  assert.strictEqual(A.controlOf(desc, 'http://192.0.2.5:49152/u/description.xml'), 'http://192.0.2.5:49152/u/ctl-avt');
  assert.strictEqual(A.controlOf('<root/>', 'http://x/'), null);
  assert.strictEqual(A.pickOwn(['http://192.0.2.61:49152/d.xml', 'http://192.0.2.36:49152/u/description.xml'], new Set(['127.0.0.1', '192.0.2.36'])), 'http://192.0.2.36:49152/u/description.xml');
  assert.strictEqual(A.pickOwn(['http://192.0.2.61:49152/d.xml'], new Set(['192.0.2.36'])), null);
});

test('a title that is the file name is recognised, a real one is not', () => {
  assert.ok(A.fileTitle('file.mp3', ''));
  assert.ok(A.fileTitle('Track 01.FLAC', ''));
  assert.ok(A.fileTitle('file', 'http://x/parts/1/file'), 'the last part of the location');
  assert.ok(A.fileTitle('Don\'t You Remember.mp3', 'http://127.0.0.1/INTERNAL/Adele/21/Don%27t%20You%20Remember.mp3'));
  assert.ok(!A.fileTitle('So Far Away', 'http://x/parts/1/file.mp3'));
  assert.ok(!A.fileTitle('', ''));
});

test('what a state wants: nothing when whole or not playing, each field it lacks otherwise', () => {
  const whole = { status: 'play', trackType: 'flac', title: 'A', artist: 'B', album: 'C', duration: 100, samplerate: '44.1 kHz', bitdepth: '16 bit', albumart: '/albumart?web=x' };
  assert.deepStrictEqual(A.wanted(whole), []);
  assert.deepStrictEqual(A.wanted(Object.assign({}, whole, { status: 'pause', trackType: '' })), [], 'not playing: nothing asked');
  assert.deepStrictEqual(A.wanted(Object.assign({}, whole, { trackType: '' })), ['trackType'], 'the YouTube case');
  assert.deepStrictEqual(A.wanted({ status: 'play', title: 'file.mp3', uri: 'http://x/file.mp3', albumart: '/albumart', trackType: 'mp3' }), ['title', 'artist', 'album', 'duration', 'samplerate', 'bitdepth', 'albumart'], 'the DLNA case');
  assert.deepStrictEqual(A.wanted(null), []);
});

test("the guards: MPD must play at the state's position; the renderer must play MPD's track", () => {
  const mpd = { song: A.parseMpd(MPD_SONG), status: A.parseMpd(MPD_STATUS) };
  assert.ok(A.mpdAgrees({ seek: 12000 }, mpd));
  assert.ok(A.mpdAgrees({ seek: 16000 }, mpd), 'within five seconds');
  assert.ok(!A.mpdAgrees({ seek: 60000 }, mpd), 'another position: another track');
  assert.ok(A.mpdAgrees({}, mpd), 'no position known: MPD\'s word');
  assert.ok(!A.mpdAgrees({ seek: 12000 }, { song: {}, status: { state: 'stop' } }), 'MPD stopped: Spotify or AirPlay plays, not MPD');
  assert.ok(!A.mpdAgrees({ seek: 12000 }, null));
  const renderer = { transport: 'PLAYING', uri: 'http://x/a%20b.mp3', meta: {} };
  assert.ok(A.rendererAgrees(renderer, { song: { file: 'http://x/a b.mp3' } }), 'the same address, decoded');
  assert.ok(A.rendererAgrees({ transport: 'PLAYING', uri: "http://127.0.0.1/INTERNAL/Adele/21/Don&apos;t You Remember.mp3", meta: {} }, { song: { file: "http://127.0.0.1/INTERNAL/Adele/21/Don't You Remember.mp3" } }), 'an address the renderer escaped once more');
  assert.strictEqual(A.plainAddress('a&amp;apos;b%20c'), "a'b c");
  assert.ok(!A.rendererAgrees(renderer, { song: { file: 'http://y/other.mp3' } }));
  assert.ok(A.rendererAgrees(renderer, null), 'MPD unknown: the renderer\'s word');
  assert.ok(!A.rendererAgrees({ transport: 'STOPPED', uri: 'http://x/a.mp3' }, null));
});

test('the fill: the YouTube track type after an automatic change, from MPD, nothing else touched', () => {
  const state = { status: 'play', service: 'ytmusic', uri: 'ytmusic/song/abc', title: 'Casa Tupka', artist: 'Dai', album: 'Album', duration: 214, seek: 12000, samplerate: '44.1 kHz', bitdepth: '32 bit', channels: 2, albumart: 'https://i.ytimg.com/vi/abc/hq.jpg', trackType: '' };
  const made = A.fill(state, { song: A.parseMpd(MPD_SONG), status: A.parseMpd(MPD_STATUS) }, null);
  assert.deepStrictEqual(made.filled, [{ field: 'trackType', from: 'mpd' }]);
  assert.strictEqual(made.state.trackType, 'YouTube');
  assert.strictEqual(made.state.title, 'Casa Tupka');
  assert.strictEqual(state.trackType, '', 'the given state is not written');
});

test('the fill: a DLNA track named by its file, made whole from MPD and the renderer, the renderer winning the names MPD lacks', () => {
  const state = { status: 'play', service: 'mpd', uri: 'http://192.0.2.9:32400/library/parts/1/file.mp3', title: 'file.mp3', artist: null, album: null, duration: 0, seek: 12000, samplerate: '', bitdepth: '', albumart: '/albumart', trackType: 'mp3' };
  const mpd = { song: { file: 'http://192.0.2.9:32400/library/parts/1/file.mp3', Time: '214' }, status: { state: 'play', elapsed: '12.3', audio: '44100:16:2' } };
  const renderer = { transport: 'PLAYING', uri: 'http://192.0.2.9:32400/library/parts/1/file.mp3', duration: 214, meta: A.parseDidl(DIDL) };
  const made = A.fill(state, mpd, renderer);
  assert.strictEqual(made.state.title, 'So Far Away');
  assert.strictEqual(made.state.artist, 'Dire Straits');
  assert.strictEqual(made.state.album, 'Brothers In Arms');
  assert.strictEqual(made.state.albumart, 'http://192.0.2.9:32400/library/metadata/1/thumb.jpg');
  assert.strictEqual(made.state.duration, 214);
  assert.strictEqual(made.state.samplerate, '44.1 kHz');
  assert.strictEqual(made.state.bitdepth, '16 bit');
  assert.strictEqual(made.state.trackType, 'mp3', 'what the state had stays');
  const from = {}; made.filled.forEach((f) => { from[f.field] = f.from; });
  assert.deepStrictEqual(from, { title: 'renderer', artist: 'renderer', album: 'renderer', albumart: 'renderer', duration: 'mpd', samplerate: 'mpd', bitdepth: 'mpd' });
  // MPD's own tags win where it has them, and a file-named title from MPD is not taken.
  const tagged = { song: Object.assign({}, mpd.song, { Title: 'From MPD', Artist: 'MPD Artist' }), status: mpd.status };
  const again = A.fill(state, tagged, renderer);
  assert.strictEqual(again.state.title, 'From MPD');
  assert.strictEqual(again.state.artist, 'MPD Artist');
  assert.strictEqual(again.state.album, 'Brothers In Arms');
  const fileNamed = { song: Object.assign({}, mpd.song, { Title: 'file.mp3' }), status: mpd.status };
  assert.strictEqual(A.fill(state, fileNamed, renderer).state.title, 'So Far Away');
  // The renderer playing another track fills none of the names.
  const other = Object.assign({}, renderer, { uri: 'http://192.0.2.9:32400/library/parts/2/other.mp3' });
  assert.strictEqual(A.fill(state, mpd, other).state.title, 'file.mp3');
  // A cover that is not an address is not taken.
  const noArt = Object.assign({}, renderer, { meta: Object.assign({}, renderer.meta, { albumart: 'thumb.jpg' }) });
  assert.strictEqual(A.fill(state, mpd, noArt).state.albumart, '/albumart');
  // Nothing wanted: the state as given, nothing filled.
  assert.deepStrictEqual(A.fill({ status: 'pause', trackType: '' }, mpd, renderer).filled, []);
});

test('MPD is asked over its socket and answered, or given up on in time', async () => {
  const server = net.createServer((socket) => {
    socket.unref();
    socket.write('OK MPD 0.24.0\n');
    socket.on('data', (d) => {
      if (String(d).indexOf('command_list_ok_begin') !== -1) socket.write(MPD_SONG + 'list_OK\n' + MPD_STATUS + 'list_OK\nOK\n');
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  server.unref();
  const port = server.address().port;
  const answer = await A.askMpd({ host: '127.0.0.1', port: port, timeoutMs: 800 });
  assert.strictEqual(answer.song.Title, 'Casa Tupka');
  assert.strictEqual(answer.status.state, 'play');
  server.close();
  const silent = net.createServer((socket) => { socket.unref(); });
  await new Promise((r) => silent.listen(0, '127.0.0.1', r));
  silent.unref();
  const started = Date.now();
  assert.strictEqual(await A.askMpd({ host: '127.0.0.1', port: silent.address().port, timeoutMs: 200 }), null);
  assert.ok(Date.now() - started < 1000, 'given up in time');
  silent.close();
  assert.strictEqual(await A.askMpd({ host: '127.0.0.1', port: 1, timeoutMs: 300 }), null, 'nothing listening');
});

test('the renderer is asked by SOAP and answered, and found by its description', async () => {
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (c) => { body += c; });
    req.on('end', () => {
      if (req.url === '/u/description.xml') { res.writeHead(200, { 'Content-Type': 'text/xml' }); return res.end('<root><device><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/u/ctl-avt</controlURL></service></serviceList></device></root>'); }
      const action = String(req.headers.soapaction || '').replace(/.*#/, '').replace(/"/g, '');
      if (req.url === '/u/ctl-avt' && RENDERER[action]) { res.writeHead(200, { 'Content-Type': 'text/xml' }); return res.end(RENDERER[action]); }
      res.writeHead(404); res.end();
    });
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  server.unref();
  const base = 'http://127.0.0.1:' + server.address().port;
  const control = await A.discoverRenderer({ search: async () => ['http://192.0.2.61:49152/d.xml', base + '/u/description.xml'], addresses: new Set(['127.0.0.1']) });
  assert.strictEqual(control, base + '/u/ctl-avt');
  const answer = await A.askRenderer(control, { timeoutMs: 800 });
  assert.strictEqual(answer.transport, 'PLAYING');
  assert.strictEqual(answer.uri, 'http://192.0.2.9:32400/library/parts/1/file.mp3');
  assert.strictEqual(answer.duration, 214);
  assert.strictEqual(answer.meta.title, 'So Far Away');
  assert.strictEqual(await A.discoverRenderer({ search: async () => ['http://192.0.2.61:49152/d.xml'], addresses: new Set(['127.0.0.1']) }), null, 'no renderer of this machine');
  server.closeAllConnections(); server.close();
  assert.strictEqual(await A.askRenderer('http://127.0.0.1:1/x', { timeoutMs: 300 }), null, 'nothing listening');
});

test('the whole: nothing wanted asks nothing; the renderer found once and dropped when it stops answering; the tally counts', async () => {
  A.resetTally();
  let mpdAsks = 0;
  const askMpd = async () => { mpdAsks += 1; return { song: A.parseMpd(MPD_SONG), status: A.parseMpd(MPD_STATUS) }; };
  const whole = { status: 'play', trackType: 'flac', title: 'A', artist: 'B', album: 'C', duration: 1, samplerate: 'x', bitdepth: 'y', channels: 2, albumart: '/albumart?web=x' };
  const untouched = await A.assure(whole, {}, { askMpd });
  assert.strictEqual(untouched.state, whole);
  assert.strictEqual(mpdAsks, 0);
  const cache = {};
  let discovered = 0;
  let rendererAsks = 0;
  const opts = { askMpd, discover: async () => { discovered += 1; return 'http://127.0.0.1:1/ctl'; }, askRenderer: async () => { rendererAsks += 1; return { transport: 'PLAYING', uri: 'https://rr3---sn-x.googlevideo.com/videoplayback?expire=1&t.YouTube', duration: 0, meta: { title: '', artist: '', album: 'From Renderer', albumart: '' } }; } };
  const yt = { status: 'play', uri: 'ytmusic/x', title: 'Casa Tupka', artist: 'Dai', album: '', duration: 214, seek: 12000, samplerate: 'x', bitdepth: 'y', albumart: 'https://i/x.jpg', trackType: '' };
  const made = await A.assure(yt, cache, opts);
  assert.strictEqual(made.state.trackType, 'YouTube');
  assert.strictEqual(made.state.album, 'From Renderer');
  assert.strictEqual(discovered, 1);
  await A.assure(yt, cache, opts);
  assert.strictEqual(discovered, 1, 'the renderer is kept between asks');
  assert.strictEqual(rendererAsks, 2);
  // Only the track type wanted: the renderer is not asked at all.
  const typeOnly = Object.assign({}, yt, { album: 'Album' });
  await A.assure(typeOnly, cache, opts);
  assert.strictEqual(rendererAsks, 2);
  // The renderer stops answering: dropped, looked for again next time.
  const gone = Object.assign({}, opts, { askRenderer: async () => null });
  await A.assure(yt, cache, gone);
  assert.strictEqual(cache.control, null);
  await A.assure(yt, cache, opts);
  assert.strictEqual(discovered, 2);
  const f = A.fact();
  assert.ok(f.count >= 4 && f.fields.indexOf('trackType') !== -1 && f.from.indexOf('mpd') !== -1 && f.at);
  A.resetTally();
});
