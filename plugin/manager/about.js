'use strict';

const DESTINATIONS = Object.freeze({
    credits: 'https://github.com/foonerd/glass_templates/graphs/contributors',
    glass: 'https://github.com/foonerd/glass',
    'glass-evo': 'https://github.com/foonerd/glass-evo',
    framework: 'https://evoframework.org',
    kofi: 'https://ko-fi.com/justanerd',
    sponsors: 'https://github.com/sponsors/foonerd'
});

function destination(name) {
    return DESTINATIONS[name] || null;
}

function qrSvg(name, encoder) {
    const url = destination(name);
    if (!url) return Promise.resolve(null);
    const QRCode = encoder || require('qrcode');
    return QRCode.toString(url, {
        type: 'svg',
        errorCorrectionLevel: 'M',
        margin: 2,
        width: 320
    });
}

module.exports = { DESTINATIONS, destination, qrSvg };