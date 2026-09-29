'use strict';
// The player's queue as the channel carries it: one item per track with the
// four fields a display reads, whatever else the player sends with them.

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
      duration: isFinite(duration) && duration > 0 ? duration : 0
    };
  });
}

module.exports = { compact };
