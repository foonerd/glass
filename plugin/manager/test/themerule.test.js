'use strict';

// The edition tag rules, replayed row by row: the tag in the album name,
// then in the folder, then the source the music plays from; a tag in the
// name winning over the source; the words and the ids; the rules' parsing.

const test = require('node:test');
const assert = require('node:assert');
const { lastEditionTag, folderNameFromUri, parseThemeTagRules, sourceTags, pickTheme, themeFolderForEdition, SOURCE_WORDS } = require('../themerule');

const RULES = 'Vinyl=1920x1080_g5_TurnTouch, Tape=1920x1080_g5_TapeTouch, SACD=1920x1080_g5_CdTouch, BR=1920x1080_g5_CassetteTouch,CD=1920x1080_g5_CdTouch\nRadio=1280x720_radio, YouTube=1280x720_tube, tidal=1280x720_hires';

test('the last bracketed tag in a name is the tag, and what follows the bracket is ignored', () => {
  assert.strictEqual(lastEditionTag('The Dark Side Of The Moon [Vinyl]'), 'Vinyl');
  assert.strictEqual(lastEditionTag('Meddle [Tape] 1971'), 'Tape');
  assert.strictEqual(lastEditionTag('[1973] The Dark Side Of The Moon [SACD] MCH'), 'SACD');
  assert.strictEqual(lastEditionTag('Dark Side Vinyl'), '', 'a word outside brackets is no tag');
  assert.strictEqual(lastEditionTag(''), '');
  assert.strictEqual(lastEditionTag(null), '');
  assert.strictEqual(lastEditionTag('[ spaced ]'), 'spaced');
});

test("the folder that holds the file is the location's second-to-last part", () => {
  assert.strictEqual(folderNameFromUri('music-library/NAS/Pink Floyd/[1973] Meddle [Vinyl] 2016/01.flac'), '[1973] Meddle [Vinyl] 2016');
  assert.strictEqual(folderNameFromUri('music-library/NAS/x%20%5BTape%5D/01.flac?x=1'), 'x [Tape]', 'decoded, the query dropped');
  assert.strictEqual(folderNameFromUri('01.flac'), '', 'no folder above the file');
  assert.strictEqual(folderNameFromUri('http://stream.example/live'), 'stream.example', 'a stream has a host, not a folder: it carries no bracket');
  assert.strictEqual(folderNameFromUri(''), '');
});

test('the rules parse by commas or new lines, the word lowercase, a folder that leaves the themes folder dropped', () => {
  const rules = parseThemeTagRules(RULES);
  assert.deepStrictEqual(rules.map((r) => r.tag), ['vinyl', 'tape', 'sacd', 'br', 'cd', 'radio', 'youtube', 'tidal']);
  assert.strictEqual(rules[0].folder, '1920x1080_g5_TurnTouch');
  assert.deepStrictEqual(parseThemeTagRules('=x, y=, nothing, ../up=../x, deep=a/b, ok = folder '), [{ tag: 'ok', folder: 'folder' }]);
  assert.deepStrictEqual(parseThemeTagRules(''), []);
  assert.deepStrictEqual(parseThemeTagRules(null), []);
});

test('a source answers to its word, its family and its id, the most particular first; one without a word to its id alone', () => {
  assert.deepStrictEqual(sourceTags('webradio'), ['radio', 'webradio']);
  assert.deepStrictEqual(sourceTags('youtube2'), ['youtube', 'youtube2']);
  assert.deepStrictEqual(sourceTags('ytmusic'), ['youtubemusic', 'ytmusic']);
  assert.deepStrictEqual(sourceTags('airplay_emulation'), ['airplay', 'airplay_emulation']);
  assert.deepStrictEqual(sourceTags('mpd'), ['local', 'mpd']);
  assert.deepStrictEqual(sourceTags('Bandcamp'), ['bandcamp'], 'a word that is the id is said once');
  assert.deepStrictEqual(sourceTags('podcast'), ['podcast']);
  assert.deepStrictEqual(sourceTags('tidal'), ['tidal']);
  // Spotify plays under two ids: the Spotify plugin and Soloist Connect.
  assert.deepStrictEqual(sourceTags('spop'), ['spotify', 'spop']);
  assert.deepStrictEqual(sourceTags('soloist_connect'), ['spotify', 'soloist_connect']);
  // Radio is a family: every plugin whose id carries the word, Radio 357 named since its id does not.
  ['radio_paradise', 'radio_browser', 'radio_fip', 'radio_france', 'personal_radio', 'calmradio', 'controradio', 'jp_radio', 'motherearthradio'].forEach((id) => {
    assert.deepStrictEqual(sourceTags(id), ['radio', id], id);
  });
  assert.deepStrictEqual(sourceTags('rad357pl'), ['radio', 'rad357pl']);
  // The tuner: FM or DAB by the station's address, then Radio, then its id.
  assert.deepStrictEqual(sourceTags('rtlsdr_radio', 'rtlsdr://fm/101.1'), ['fm', 'radio', 'rtlsdr_radio']);
  assert.deepStrictEqual(sourceTags('rtlsdr_radio', 'rtlsdr://dab/12B/BBC%20Radio%204'), ['dab', 'radio', 'rtlsdr_radio']);
  assert.deepStrictEqual(sourceTags('rtlsdr_radio', 'rtlsdr://favorites'), ['radio', 'rtlsdr_radio'], 'an address that names neither');
  assert.deepStrictEqual(sourceTags('rtlsdr_radio'), ['radio', 'rtlsdr_radio']);
  assert.deepStrictEqual(sourceTags('webradio', 'rtlsdr://fm/1'), ['radio', 'webradio'], 'the address counts for the tuner alone');
  assert.deepStrictEqual(sourceTags(''), []);
  assert.deepStrictEqual(sourceTags(undefined), []);
  Object.keys(SOURCE_WORDS).forEach((id) => assert.strictEqual(SOURCE_WORDS[id], SOURCE_WORDS[id].toLowerCase(), id));
});

test('the families pick: FM before Radio before the id, Spotify for both ways it plays, a radio plugin by the family word', () => {
  const rules = 'Radio=1280x720_radio, FM=1280x720_fm, Spotify=1280x720_spot, rtlsdr_radio=1280x720_tuner';
  assert.deepStrictEqual(pickTheme('', 'rtlsdr://fm/101.1', 'rtlsdr_radio', rules), { folder: '1280x720_fm', tag: 'fm', by: 'source' });
  assert.deepStrictEqual(pickTheme('', 'rtlsdr://dab/12B/x', 'rtlsdr_radio', rules), { folder: '1280x720_radio', tag: 'radio', by: 'source' }, 'no DAB rule: the family word');
  assert.deepStrictEqual(pickTheme('', 'rtlsdr://dab/12B/x', 'rtlsdr_radio', 'rtlsdr_radio=1280x720_tuner'), { folder: '1280x720_tuner', tag: 'rtlsdr_radio', by: 'source' }, 'no word rule: the id');
  assert.deepStrictEqual(pickTheme('', 'http://x', 'radio_paradise', rules).folder, '1280x720_radio');
  assert.deepStrictEqual(pickTheme('', 'spotify:track:1', 'soloist_connect', rules).folder, '1280x720_spot');
  assert.deepStrictEqual(pickTheme('', 'spotify:track:1', 'spop', rules).folder, '1280x720_spot');
  assert.deepStrictEqual(pickTheme('Show [Vinyl]', 'rtlsdr://fm/1', 'rtlsdr_radio', rules + ', Vinyl=1280x720_vinyl').by, 'album', 'a tag in the name still wins');
});

test('the pick: the album tag first, then the folder tag, then the source; a tag in the name wins over the source', () => {
  // As before this cut: the album, then the folder.
  assert.deepStrictEqual(pickTheme('Meddle [Tape]', 'music-library/x/[Vinyl]/a.flac', 'mpd', RULES), { folder: '1920x1080_g5_TapeTouch', tag: 'tape', by: 'album' });
  assert.deepStrictEqual(pickTheme('Meddle', 'music-library/x/[1973] Meddle [Vinyl] 2016/a.flac', 'mpd', RULES), { folder: '1920x1080_g5_TurnTouch', tag: 'vinyl', by: 'folder' });
  assert.deepStrictEqual(pickTheme('MEDDLE [sacd]', '', '', RULES).folder, '1920x1080_g5_CdTouch', 'without regard to case');
  // A web radio stream and a YouTube video: no tag of the listener's making, the source stands in.
  assert.deepStrictEqual(pickTheme('', 'http://stream.example/live', 'webradio', RULES), { folder: '1280x720_radio', tag: 'radio', by: 'source' });
  assert.deepStrictEqual(pickTheme('Some channel', 'youtube2/video@videoId=abc', 'youtube2', RULES), { folder: '1280x720_tube', tag: 'youtube', by: 'source' });
  // A source without a word of its own answers to its id.
  assert.deepStrictEqual(pickTheme('Album', 'tidal://track/1', 'tidal', RULES), { folder: '1280x720_hires', tag: 'tidal', by: 'source' });
  // A tag in the name wins over the source.
  assert.deepStrictEqual(pickTheme('Live set [Vinyl]', 'http://stream.example/live', 'webradio', RULES).by, 'album');
  // No rule for the source, no tag: nothing picked, the home theme stays.
  assert.deepStrictEqual(pickTheme('Album', 'spop:track:1', 'spop', RULES), { folder: '', tag: '', by: '' });
  assert.deepStrictEqual(pickTheme('Album', 'music-library/x/a.flac', 'mpd', RULES), { folder: '', tag: '', by: '' }, 'no Local rule: local files keep the home theme');
  assert.deepStrictEqual(pickTheme('Album', 'music-library/x/a.flac', 'mpd', RULES + ', Local=1280x720_files'), { folder: '1280x720_files', tag: 'local', by: 'source' });
  // No rules at all: null, as the plugin has long read it.
  assert.strictEqual(pickTheme('Meddle [Tape]', '', 'mpd', ''), null);
  // The folder alone, the old contract, with and without a service.
  assert.strictEqual(themeFolderForEdition('Meddle [Tape]', '', RULES), '1920x1080_g5_TapeTouch');
  assert.strictEqual(themeFolderForEdition('', 'http://stream.example/live', RULES, 'webradio'), '1280x720_radio');
  assert.strictEqual(themeFolderForEdition('', 'http://stream.example/live', RULES), '', 'no service given, no source match');
  assert.strictEqual(themeFolderForEdition('x', 'y', ''), null);
});
