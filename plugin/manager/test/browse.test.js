'use strict';
const test = require('node:test');
const assert = require('node:assert');
const { sources, answer, playable, played, itemOf, Kept, MOST } = require('../browse');

test('the sources wear the stock icons by name, a service its own picture, and the line is a browse of nothing', () => {
  const line = sources([
    { albumart: '/albumart?sourceicon=music_service/mpd/favouritesicon.png', name: 'Favourites', uri: 'favourites', plugin_type: '', plugin_name: '' },
    { albumart: '/albumart?sourceicon=music_service/mpd/musiclibraryicon.png', name: 'Music Library', uri: 'music-library', plugin_type: 'music_service', plugin_name: 'mpd' },
    { albumart: '/albumart?sourceicon=music_service/spop/spotify.png', name: 'Spotify', uri: 'spotify', plugin_type: 'music_service', plugin_name: 'spop' },
    { icon: 'fa fa-globe', name: 'Web Radio', uri: 'radio', plugin_type: 'music_service', plugin_name: 'webradio' },
    { name: 'Odd', uri: 'odd://' },
    null,
    { uri: 'nameless' }
  ], 7);
  assert.equal(line.kind, 'browse');
  assert.equal(line.uri, '');
  assert.equal(line.title, 'Browse');
  assert.equal(line.serial, 7);
  assert.equal(line.items.length, 5);
  assert.deepEqual(line.items.map((i) => i.icon), ['heart', 'music', 'cloud', 'globe', 'cloud'], 'an icon for every source, worn until its own picture is in');
  assert.equal(line.items[2].art, '/albumart?sourceicon=music_service/spop/spotify.png', "a service's own logo is a picture");
  assert.equal(line.items[0].art, '', 'a stock picture is an icon, not a picture');
  assert.equal(line.items[1].service, 'mpd');
  assert.ok(line.items.every((i) => i.type === 'source' && !i.leaf && !i.heading));
});

test("a location's lists become one list of items, headings between several, leaves marked, covers kept and stock pictures turned to icons", () => {
  const nav = {
    prev: { uri: 'music-library' },
    lists: [
      { title: 'Folders', availableListViews: ['list'], items: [
        { service: 'mpd', type: 'folder', title: 'NAS', albumart: '/albumart?sourceicon=music_service/mpd/musiclibraryicon.png', uri: 'music-library/NAS' },
        { service: 'mpd', type: 'remdisk', title: 'USB', icon: 'fa fa-usb', uri: 'music-library/USB' }
      ] },
      { title: 'Songs', items: [
        { service: 'mpd', type: 'song', title: 'One', artist: 'A', album: 'X', albumart: '/albumart?path=%2Fx', uri: 'mnt/x/1.flac', duration: 100 },
        { service: 'webradio', type: 'webradio', title: 'Radio', artist: 'MP3 (128 kbps)', album: '', icon: 'fa fa-microphone', uri: 'http://stream' },
        { service: 'mpd', type: 'song', name: 'Named', uri: 'mnt/x/2.flac', duration: '12' },
        'not an item'
      ] }
    ]
  };
  const line = answer('music-library/NAS', nav, 2, 'NAS');
  assert.equal(line.uri, 'music-library/NAS');
  assert.equal(line.title, 'NAS');
  assert.equal(line.prev, 'music-library');
  assert.equal(line.info, null);
  assert.deepEqual(line.items.map((i) => i.title), ['Folders', 'NAS', 'USB', 'Songs', 'One', 'Radio', 'Named']);
  assert.ok(line.items[0].heading && line.items[3].heading);
  assert.deepEqual(line.items.map((i) => i.icon), ['', 'music', 'usb', '', 'music', 'mic', 'music']);
  assert.equal(line.items[4].art, '/albumart?path=%2Fx');
  assert.equal(line.items[4].subtitle, 'A · X');
  assert.equal(line.items[4].duration, 100);
  assert.ok(line.items[4].leaf && line.items[5].leaf && line.items[6].leaf);
  assert.ok(!line.items[1].leaf && !line.items[2].leaf);
  assert.equal(line.items[5].subtitle, 'MP3 (128 kbps)');
  assert.equal(line.items[6].duration, 12);
});

test("an album's header is kept as the page's title where the face gives none, and one list makes no heading", () => {
  const nav = {
    prev: { uri: 'albums://' },
    info: { uri: 'albums://A/X', service: 'mpd', artist: 'A', album: 'X', albumart: '/albumart?web=A/X', type: 'album' },
    lists: [{ title: 'Tracks', items: [{ type: 'song', title: 'One', uri: 'mnt/1', service: 'mpd' }] }]
  };
  const line = answer('albums://A/X', nav, 1, '');
  assert.equal(line.title, 'X');
  assert.equal(line.info.art, '/albumart?web=A/X');
  assert.equal(line.info.subtitle, 'A · X');
  assert.equal(line.items.length, 1, 'no heading for one list');
  assert.equal(answer('x', null, 0, '').items.length, 0);
  assert.equal(answer('x', { lists: 'no' }, 0, 'T').title, 'T');
});

test('what a tap plays is the leaves of the list with the tapped place among them', () => {
  const items = [
    itemOf({ type: 'folder', title: 'F', uri: 'f', service: 'mpd' }),
    itemOf({ type: 'song', title: 'One', uri: '1', service: 'mpd', albumart: '/albumart?path=1' }),
    itemOf({ type: 'title', title: 'Heading' }),
    itemOf({ type: 'song', title: 'Two', uri: '2', service: 'mpd' })
  ];
  const tapped = playable(items, 3);
  assert.deepEqual(tapped.list.map((i) => i.uri), ['1', '2']);
  assert.equal(tapped.index, 1);
  assert.equal(tapped.list[0].albumart, '/albumart?path=1');
  assert.equal(tapped.list[1].albumart, undefined);
  assert.equal(playable(items, 0).index, -1, 'a folder is no leaf');
  assert.deepEqual(played({ uri: 'u', service: 's', title: 't', type: 'song', art: '/a' }), { uri: 'u', service: 's', title: 't', type: 'song', albumart: '/a' });
});

test('the kept answers are by location, the newest kept, the oldest let go beyond the most', () => {
  const kept = new Kept(2);
  kept.put({ uri: 'a', serial: 1 });
  kept.put({ uri: 'b', serial: 2 });
  kept.put({ uri: 'a', serial: 3 });
  assert.equal(kept.get('a').serial, 3);
  assert.equal(kept.lines.length, 2);
  kept.put({ uri: 'c', serial: 4 });
  assert.equal(kept.get('b'), null, 'the oldest went');
  assert.equal(kept.get('c').serial, 4);
  kept.put(null);
  assert.equal(kept.lines.length, 2);
});

test('an answer carries at most the most items', () => {
  const many = { lists: [{ items: Array.from({ length: MOST + 5 }, (_, i) => ({ type: 'song', title: String(i), uri: String(i) })) }] };
  assert.equal(answer('x', many, 0, '').items.length, MOST);
});
