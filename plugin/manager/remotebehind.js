'use strict';
// Whether a remote display is behind the latest release of what it is.
// A remote says the Glass it is built from and, where it is a display
// built with a face, the face's name and version; the Manager knows the
// latest release of Glass and of glass-evo from its own looks. The Manager
// only says it: a remote upgrades itself from its own settings page.
// Pure.

const { compareVersions } = require('./update');

const VERSION = /^\d+\.\d+\.\d+$/;

// What a remote is a release of: its face's product at the face's version
// ("glass-evo 0.1.28"), else Glass at the release it names.
function productOf(remote) {
  const r = remote || {};
  const face = /^([A-Za-z][A-Za-z0-9_-]{0,31}) (\d+\.\d+\.\d+)$/.exec(String(r.face || '').trim());
  if (face) return { product: face[1], version: face[2] };
  return { product: 'Glass', version: VERSION.test(String(r.release || '')) ? String(r.release) : null };
}

// Where a remote stands against the releases last seen, by product name:
// `behind` true or false, or null where it cannot be said: a remote that
// names no version, a product no release was seen of, or a release seen
// that is a test release, which no remote is offered.
function standing(remote, latest) {
  const is = productOf(remote);
  const seen = (latest || {})[is.product];
  if (!is.version || !seen || !VERSION.test(String(seen.version || '')) || seen.prerelease) {
    return { product: is.product, version: is.version, latest: null, behind: null };
  }
  return { product: is.product, version: is.version, latest: seen.version, behind: compareVersions(seen.version, is.version) > 0 };
}

module.exports = { productOf: productOf, standing: standing };
