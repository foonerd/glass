'use strict';

// Performance profiles: the settings that decide what a theme costs,
// bundled, and the profile a board gets when the choice is left to Glass.

const PROFILES = {
  full: { frameRate: 60, rotationQuality: 'high', rotationFps: 15, transitions: true },
  standard: { frameRate: 30, rotationQuality: 'medium', rotationFps: 8, transitions: true },
  light: { frameRate: 20, rotationQuality: 'low', rotationFps: 4, transitions: true },
  minimal: { frameRate: 15, rotationQuality: 'low', rotationFps: 4, transitions: false },
};
const NAMES = ['auto', 'full', 'standard', 'light', 'minimal', 'custom'];

// The board's class from its model string (the device tree on a
// Raspberry Pi), its processor count and its architecture.
function boardClass(model, cores, arch) {
  const m = String(model || '').toLowerCase();
  const a = String(arch || '').toLowerCase();
  if (/raspberry pi 5|compute module 5|raspberry pi 500/.test(m)) return 'pi5';
  if (/raspberry pi 4|compute module 4|raspberry pi 400/.test(m)) return 'pi4';
  if (/raspberry pi zero 2/.test(m)) return 'zero2';
  if (/raspberry pi 3|compute module 3/.test(m)) return 'pi3';
  if (/raspberry pi zero|raspberry pi 2|raspberry pi model|raspberry pi compute module\b/.test(m)) return 'pi1';
  if (/x86_64|amd64/.test(a)) return 'x64';
  if (/raspberry pi/.test(m)) return 'pi';
  return (cores || 1) >= 4 ? 'other4' : 'other';
}

// The profile a board is given under "auto". A large theme on a small
// board takes the row below.
function autoProfile(cls, themeHeight) {
  const big = (themeHeight || 0) > 720;
  switch (cls) {
    case 'pi5':
    case 'x64':
    case 'pi4':
    case 'other4':
      return 'standard';
    case 'pi3':
      return big ? 'minimal' : 'light';
    case 'zero2':
    case 'pi1':
    case 'pi':
      return 'minimal';
    default:
      return 'light';
  }
}

// Which profile a set of values is, or `custom`.
function nameOf(values) {
  const v = values || {};
  for (const name of Object.keys(PROFILES)) {
    const p = PROFILES[name];
    if (Number(v.frameRate) === p.frameRate && String(v.rotationQuality) === p.rotationQuality && Number(v.rotationFps) === p.rotationFps && !!v.transitions === p.transitions) {
      return name;
    }
  }
  return 'custom';
}

module.exports = { PROFILES, NAMES, boardClass, autoProfile, nameOf };
