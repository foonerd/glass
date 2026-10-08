// The report a capture ends in: its text from the head lines, the diagnosis
// and the sheet; the forms it is handed over in, a forum post, a GitHub
// issue's link, a file. Pure, and one script for the page and for its
// tests: the Manager's page loads it as GlassReport.
(function (root, make) {
  if (typeof module === 'object' && module.exports) module.exports = make();
  else root.GlassReport = make();
})(this, function () {
  'use strict';

  // A forum post's size: Discourse allows 32,000 characters; room is left
  // for the poster's own words around the report.
  var FORUM_MAX = 30000;
  // What a link to a new GitHub issue may carry, URL-encoded.
  var ISSUE_MAX = 6000;

  // The report: the head lines, a blank line, the diagnosis, the sheet.
  // `rest` is what follows the head, for an issue's fenced block.
  function compose(head, diagnosisText, sheetText) {
    var rest = (diagnosisText ? String(diagnosisText).replace(/\n*$/, '\n') + '\n' : '') + (sheetText || '');
    return { text: head.join('\n') + '\n\n' + rest, rest: rest };
  }

  // The report for a forum post: whole where it fits, else cut at a line's
  // end with `note` in the cut's place, which says where the whole is.
  function forForum(text, note, max) {
    max = max || FORUM_MAX;
    text = String(text || '');
    if (text.length <= max) return { text: text, trimmed: false };
    var room = Math.max(1, max - String(note).length - 2);
    var cut = text.lastIndexOf('\n', room);
    if (cut < 1) cut = room;
    return { text: text.slice(0, cut) + '\n\n' + note, trimmed: true };
  }

  // The body of a GitHub issue opened by a link: the report where the link
  // can carry it, else the head and `pasteNote`, the whole being on the
  // clipboard either way.
  function issueBody(head, rest, pasteNote, max) {
    var body = head.join('\n') + '\n\n```\n' + rest + '```\n';
    return encodeURIComponent(body).length > (max || ISSUE_MAX) ? head.join('\n') + '\n\n' + pasteNote : body;
  }

  // The file a downloaded report is saved as: the player's name and the
  // minute, in characters every file system takes.
  function fileName(host, whenMs) {
    var d = new Date(whenMs);
    var two = function (n) { return ('0' + n).slice(-2); };
    var stamp = d.getFullYear() + '-' + two(d.getMonth() + 1) + '-' + two(d.getDate()) + '-' + two(d.getHours()) + '-' + two(d.getMinutes());
    var name = String(host || '').replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '') || 'player';
    return 'glass-report-' + name + '-' + stamp + '.txt';
  }

  return { compose: compose, forForum: forForum, issueBody: issueBody, fileName: fileName, FORUM_MAX: FORUM_MAX, ISSUE_MAX: ISSUE_MAX };
});
