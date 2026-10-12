'use strict';
// The player's queue as the channel carries it: one item per track with the
// fields a display reads, whatever else the player sends with them: the
// four of the next-track rows and the queue's progress, and from 0.9.39
// the track's location and its cover's, for a changer's discs.

function text(value) {
  return typeof value === 'string' ? value.trim() : '';
}

// The items of a queue the player pushed, `[]` for anything that is not a
// list. A track's title is its `title`, or its `name` when it has none.
function compact(queue) {
  if (!Array.isArray(queue)) return [];
  return queue.map(function (item) {
    var it = item && typeof item === 'object' ? item : {};
    var duration = Number(it.duration);
    return {
      title: text(it.title) || text(it.name),
      artist: text(it.artist),
      album: text(it.album),
      duration: isFinite(duration) && duration > 0 ? duration : 0,
      uri: text(it.uri),
      albumart: text(it.albumart)
    };
  });
}

// Whether `address` is the cover of an item of the queue as compacted:
// a page may ask the player's Manager for a queue item's cover at an
// address the player itself reported, and for no other.
function hasCover(items, address) {
  var wanted = String(address || '');
  if (!wanted || !Array.isArray(items)) return false;
  return items.some(function (item) { return item && item.albumart === wanted; });
}

module.exports = { compact, hasCover };
