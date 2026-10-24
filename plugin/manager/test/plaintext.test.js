'use strict';

// Names made plain: the exact shapes the sources that write markup emit,
// as read from their code (the Bandcamp family's badges, icons, pictures
// and coloured options, the Jellyfin and YouTube Music spans), entities,
// and the glued badge a state's title can carry.

const test = require('node:test');
const assert = require('node:assert');
const { name, plain, state, badgeIcon, decodeEntities } = require('../plaintext');

test('a badge in a span before a name goes, and stays as the kind', () => {
  const r = name("<span style='color: #999; font-size: 10px; padding-right: 8px;'>TRACK</span>Shadow of the Day");
  assert.deepStrictEqual(r, { text: 'Shadow of the Day', badge: 'track', marked: true });
  assert.deepStrictEqual(name("<span style='color: #999; font-size: 10px; padding-right: 8px;'>ALBUM</span>Tom Waits - Alice").badge, 'album');
  assert.strictEqual(name("<span style='color: #b38536; font-size: 10px; padding-right: 8px;'>NON-PLAYABLE</span>Preorder").badge, 'non-playable');
  // A span at the head holding more than a word of capitals is no badge: its text stays.
  assert.deepStrictEqual(name("<span style='color: #ccc; margin-right: 5px;'>Genre: </span>Rock"), { text: 'Genre: Rock', badge: '', marked: true });
  assert.strictEqual(plain("<span style='color: #54c688; font-weight: bold;'}>Selected option</span>"), 'Selected option');
  assert.strictEqual(plain("<span style='color: #7a848e;'>Next page</span>"), 'Next page');
  assert.strictEqual(plain("<span style='text-transform: capitalize;'>mixes</span>"), 'mixes');
});

test('icons and pictures before a title go, and the words keep one space between them', () => {
  assert.strictEqual(plain('<img src="/albumart?sourceicon=music_service%2Fbandcamp%2Fdist%2Fassets%2Fimages%2Fbandcamp.svg" style="width: 23px; height: 23px; margin-right: 8px; margin-top: -3px;" />Discover'), 'Discover');
  assert.strictEqual(plain('<i class="fa fa-user" style="padding-right: 15px;"></i>Artists'), 'Artists');
  assert.strictEqual(plain("<i class='fa fa-list' style='margin-right: 3px; scale: 0.9;'></i> My Sets"), 'My Sets');
  assert.strictEqual(plain('<img src="https://x/y.jpg" style="border-radius: 50%; width: 48px; height: 48px; margin-right: 12px;" /> My Profile'), 'My Profile');
  assert.strictEqual(plain('<b>Bold</b>and<br>broken'), 'Bold and broken');
});

test('entities decode, by name and by number, and what is not an entity stays', () => {
  assert.strictEqual(plain('Tom &amp; Jerry'), 'Tom & Jerry');
  assert.strictEqual(plain('Don&#39;t Stop &quot;Now&quot; &#x2014; Live'), 'Don\'t Stop "Now" — Live');
  assert.strictEqual(plain('Caf&eacute; del Mar &nbsp; Vol. 2'), 'Café del Mar Vol. 2');
  assert.strictEqual(plain('AC&DC & Co'), 'AC&DC & Co', 'a bare ampersand is a bare ampersand');
  assert.strictEqual(plain('&unknown; stays'), '&unknown; stays');
  assert.strictEqual(decodeEntities('&#0;&#9999999;'), '&#0;&#9999999;', 'numbers outside the range stay');
});

test('a plain name is untouched, and the glued badge comes off only for the services that glue it, before a capitalised word', () => {
  const r = name('Make It Better');
  assert.deepStrictEqual(r, { text: 'Make It Better', badge: '', marked: false });
  assert.strictEqual(plain('  spaced   out  '), 'spaced out');
  assert.deepStrictEqual(name('TRACKMake It Better', 'bandcamp'), { text: 'Make It Better', badge: 'track', marked: false });
  assert.strictEqual(plain('ALBUMSongs of Faith', 'Bandcamp'), 'Songs of Faith');
  assert.strictEqual(plain('TRACKMake It Better', 'mpd'), 'TRACKMake It Better', 'another service keeps its name');
  assert.strictEqual(plain('TRACKS OF THE YEAR', 'bandcamp'), 'TRACKS OF THE YEAR', 'a word of capitals is no glued badge');
  assert.strictEqual(plain('ALBUMS', 'bandcamp'), 'ALBUMS');
  assert.strictEqual(plain('Tracks', 'bandcamp'), 'Tracks');
  assert.strictEqual(plain(null), '');
  assert.strictEqual(plain(42), '42');
});

test("a state's title, artist and album are made plain on a copy, the rest as the player sent it", () => {
  const given = { status: 'play', service: 'bandcamp', uri: 'bandcamp/track/x?a=1&b=2', title: 'TRACKMake It Better', artist: 'Hondenfokker &amp; Friends', album: '<span>Older</span>', albumart: '/albumart?web=a&b' };
  const r = state(given);
  assert.strictEqual(r.cleared, 3);
  assert.deepStrictEqual(r.state, { status: 'play', service: 'bandcamp', uri: 'bandcamp/track/x?a=1&b=2', title: 'Make It Better', artist: 'Hondenfokker & Friends', album: 'Older', albumart: '/albumart?web=a&b' });
  assert.strictEqual(given.title, 'TRACKMake It Better', 'the given state is not written');
  const plainOne = state({ status: 'play', service: 'mpd', title: 'Older', artist: 'George Michael' });
  assert.strictEqual(plainOne.cleared, 0);
  assert.deepStrictEqual(state(null), { state: null, cleared: 0 });
  assert.deepStrictEqual(state({ status: 'stop', title: '' }).state, { status: 'stop', title: '' });
});

test('what was cleared is counted, with the services that sent it', () => {
  const { markupFact, resetTally } = require('../plaintext');
  resetTally();
  assert.deepStrictEqual(markupFact(), { count: 0, services: [], at: null });
  plain('<span>Older</span>', 'bandcamp');
  plain('Tom &amp; Jerry', 'mixcloud');
  plain('TRACKMake It Better', 'bandcamp');
  plain('A plain name', 'mpd');
  state({ status: 'play', service: 'soundcloud', title: '<b>x</b>', artist: 'y' });
  const fact = markupFact();
  assert.strictEqual(fact.count, 4);
  assert.deepStrictEqual(fact.services, ['bandcamp', 'mixcloud', 'soundcloud']);
  assert.ok(fact.at);
  resetTally();
});

test('a badge names the icon the face draws for it', () => {
  assert.strictEqual(badgeIcon('track'), 'music');
  assert.strictEqual(badgeIcon('album'), 'disc-3');
  assert.strictEqual(badgeIcon('artist'), 'user-round');
  assert.strictEqual(badgeIcon('label'), 'tag');
  assert.strictEqual(badgeIcon('non-playable'), '');
  assert.strictEqual(badgeIcon(''), '');
});
