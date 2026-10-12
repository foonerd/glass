'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const about = require('../about');

test('About QR destinations are explicit and unknown names are rejected', async function () {
    assert.deepEqual(Object.keys(about.DESTINATIONS), ['credits', 'glass', 'glass-evo', 'framework', 'kofi', 'sponsors']);
    assert.equal(about.destination('credits'), 'https://github.com/foonerd/glass_templates/graphs/contributors');
    assert.equal(about.destination('glass'), 'https://github.com/foonerd/glass');
    assert.equal(about.destination('glass-evo'), 'https://github.com/foonerd/glass-evo');
    assert.equal(about.destination('framework'), 'https://evoframework.org');
    assert.equal(about.destination('kofi'), 'https://ko-fi.com/justanerd');
    assert.equal(about.destination('sponsors'), 'https://github.com/sponsors/foonerd');
    assert.equal(about.destination('https://attacker.invalid'), null);
    assert.equal(await about.qrSvg('missing'), null);
});

test('About QR output is a valid SVG carrying a real QR matrix', async function () {
    let encoded;
    const encoder = {
        toString: function (url, options) {
            encoded = { url: url, options: options };
            return Promise.resolve('<svg></svg>');
        }
    };
    assert.equal(await about.qrSvg('glass', encoder), '<svg></svg>');
    assert.deepEqual(encoded, {
        url: 'https://github.com/foonerd/glass',
        options: { type: 'svg', errorCorrectionLevel: 'M', margin: 2, width: 320 }
    });
});