'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { advanced } = require('../statenow');

test('a playing state moves on by the time since it was kept, and no further than the track', () => {
  const kept = { status: 'play', title: 'One', seek: 10000, duration: 200 };
  assert.deepEqual(advanced(kept, 1000, 6000), Object.assign({}, kept, { seek: 15000 }));
  assert.equal(kept.seek, 10000, 'the kept state is not changed');
  assert.equal(advanced(kept, 1000, 1000000).seek, 200000, 'capped at the end of the track');
  assert.equal(advanced(kept, 6000, 1000), kept, 'a clock that went back changes nothing');
  assert.equal(advanced({ status: 'pause', seek: 10000 }, 1000, 6000).seek, 10000, 'a paused player stays where it is');
  assert.equal(advanced({ status: 'play', title: 'x' }, 1000, 6000).title, 'x', 'no position, nothing to move');
  assert.equal(advanced(kept, undefined, 6000), kept, 'no stamp, nothing to move');
  assert.equal(advanced(null, 1, 2), null);
});
