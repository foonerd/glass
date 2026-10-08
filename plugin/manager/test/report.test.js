'use strict';

const test = require('node:test');
const assert = require('node:assert');
const R = require('../report');

const HEAD = ['Glass report: The forecast is missing', 'What I did and what I saw: waited', 'Reproduced between Oct 08 07:40:00 and Oct 08 07:41:30, with Glass logging at verbose (otherwise info).', 'System log: http://logs.example.org/x.html'];

test('the report is the head, a blank line, the diagnosis and the sheet, the diagnosis ending in one blank line', () => {
  const made = R.compose(HEAD, 'Diagnosis\n  Checked: a\n', 'volumio · Glass 0.9.8\n\nPlayer\n  Glass: 0.9.8\n');
  assert.strictEqual(made.text, HEAD.join('\n') + '\n\nDiagnosis\n  Checked: a\n\nvolumio · Glass 0.9.8\n\nPlayer\n  Glass: 0.9.8\n');
  assert.strictEqual(made.rest, 'Diagnosis\n  Checked: a\n\nvolumio · Glass 0.9.8\n\nPlayer\n  Glass: 0.9.8\n');
  assert.strictEqual(R.compose(HEAD, '', 'sheet\n').text, HEAD.join('\n') + '\n\nsheet\n', 'no diagnosis, no blank line for it');
  assert.strictEqual(R.compose(HEAD, 'Diagnosis\n\n\n', '').text, HEAD.join('\n') + '\n\nDiagnosis\n\n', 'extra blank lines after the diagnosis are one');
});

test('a forum post carries the whole report where it fits, else the start of it and a word on where the whole is', () => {
  const short = R.forForum('a short report\nof two lines', '[cut]');
  assert.deepStrictEqual(short, { text: 'a short report\nof two lines', trimmed: false });
  const lines = [];
  for (let i = 0; i < 4000; i++) lines.push('line ' + i + ' of the sheet');
  const long = lines.join('\n');
  assert.ok(long.length > R.FORUM_MAX);
  const post = R.forForum(long, '[The sheet is in the downloaded report.]');
  assert.strictEqual(post.trimmed, true);
  assert.ok(post.text.length <= R.FORUM_MAX, 'fits a post: ' + post.text.length);
  assert.ok(post.text.endsWith('\n\n[The sheet is in the downloaded report.]'));
  assert.ok(/of the sheet\n\n\[/.test(post.text), 'cut at a line end');
  const tiny = R.forForum('abcdefghij', '[n]', 8);
  assert.deepStrictEqual(tiny, { text: 'abc\n\n[n]', trimmed: true }, 'with no line end to cut at, cut where there is room');
});

test('an issue by link carries the report in a fenced block while the link can, else the head and the paste note', () => {
  const body = R.issueBody(HEAD, 'Diagnosis\n  Checked: a\n\nsheet\n', 'paste it');
  assert.strictEqual(body, HEAD.join('\n') + '\n\n```\nDiagnosis\n  Checked: a\n\nsheet\n```\n');
  const big = R.issueBody(HEAD, 'x'.repeat(10000), 'The full report is on your clipboard: paste it here.');
  assert.strictEqual(big, HEAD.join('\n') + '\n\nThe full report is on your clipboard: paste it here.');
  assert.strictEqual(R.issueBody(HEAD, 'small', 'paste', 10), HEAD.join('\n') + '\n\npaste', 'the limit is the encoded length');
});

test('the downloaded report is named by the player and the minute, in safe characters', () => {
  const at = new Date(2026, 9, 8, 7, 5).getTime();
  assert.strictEqual(R.fileName('volumio', at), 'glass-report-volumio-2026-10-08-07-05.txt');
  assert.strictEqual(R.fileName('my player/one', at), 'glass-report-my-player-one-2026-10-08-07-05.txt');
  assert.strictEqual(R.fileName('', at), 'glass-report-player-2026-10-08-07-05.txt');
});
