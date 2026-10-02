'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { memoryOf, pluginsOf } = require('../playerfacts');

test('the memory is what /proc/meminfo says, in megabytes', () => {
  const text = 'MemTotal:        1964512 kB\nMemFree:          412300 kB\nMemAvailable:    1105920 kB\nBuffers:           52000 kB\nSwapTotal:        524284 kB\nSwapFree:         421884 kB\n';
  assert.deepEqual(memoryOf(text), { totalMb: 1918, availableMb: 1080, usedMb: 838, swapUsedMb: 100 });
  assert.deepEqual(memoryOf('MemTotal: 8000000 kB\nMemAvailable: 6000000 kB\n'), { totalMb: 7813, availableMb: 5859, usedMb: 1953, swapUsedMb: 0 }, 'no swap is none in use');
  assert.equal(memoryOf(''), null);
  assert.equal(memoryOf('MemTotal: 1000 kB\n'), null, 'a kernel that does not say what is available');
  assert.equal(memoryOf(null), null);
});

test('the other plugins: what is installed, whether it is on, and whether it is in the audio path', () => {
  const pkg = (o) => JSON.stringify(o);
  const installed = [
    { category: 'user_interface', name: 'glass', package: pkg({ name: 'glass', version: '0.8.2', volumio_info: { prettyName: 'Glass', has_alsa_contribution: true } }) },
    { category: 'music_service', name: 'spop', package: pkg({ name: 'spop', version: '4.2.1', volumio_info: { prettyName: 'Spotify' } }) },
    { category: 'audio_interface', name: 'fusiondsp', package: pkg({ name: 'fusiondsp', version: '1.1.4', volumio_info: { prettyName: 'FusionDsp', has_alsa_contribution: true } }) },
    { category: 'user_interface', name: 'touch_display', package: pkg({ name: 'touch_display', version: '3.5.5', volumio_info: { prettyName: 'Touch Display' } }) },
    { category: 'system_hardware', name: 'broken', package: '{ not json' }
  ];
  const registry = {
    music_service: { spop: { enabled: { type: 'boolean', value: true }, status: { type: 'string', value: 'STARTED' } } },
    audio_interface: { fusiondsp: { enabled: { type: 'boolean', value: true }, status: { type: 'string', value: 'STARTED' } } },
    user_interface: { touch_display: { enabled: { type: 'boolean', value: false }, status: { type: 'string', value: 'STOPPED' } }, glass: { enabled: { value: true } } }
  };
  const found = pluginsOf(installed, registry);
  assert.deepEqual(found.map((p) => p.title), ['FusionDsp', 'broken', 'Spotify', 'Touch Display'], 'the audio path first, then by name; Glass is not its own other');
  assert.deepEqual(found[0], { category: 'audio_interface', name: 'fusiondsp', title: 'FusionDsp', version: '1.1.4', enabled: true, running: true, audioPath: true });
  assert.deepEqual([found[2].enabled, found[2].running, found[2].audioPath], [true, true, false]);
  assert.deepEqual([found[3].enabled, found[3].running], [false, false]);
  assert.deepEqual(found[1], { category: 'system_hardware', name: 'broken', title: 'broken', version: '', enabled: false, running: false, audioPath: false }, 'a package that cannot be read still shows, by its folder');
  assert.deepEqual(pluginsOf([], registry), []);
  assert.deepEqual(pluginsOf(null, null), []);
});
