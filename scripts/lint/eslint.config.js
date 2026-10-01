// The lint scripts/check.sh runs over the plugin's scripts and the pages'
// inline scripts: one rule, no name used that nothing defines. A script
// with such a name parses and passes its tests, and throws when the line
// runs. The globals are listed here so the lint needs ESLint and nothing
// else.
const names = (list) => Object.fromEntries(list.map((name) => [name, 'readonly']));

const shared = names([
  'console', 'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval',
  'fetch', 'URL', 'URLSearchParams', 'AbortController', 'AbortSignal',
  'TextDecoder', 'TextEncoder', 'WebAssembly', 'atob', 'btoa',
  'structuredClone', 'performance', 'queueMicrotask'
]);

const node = names([
  'require', 'module', 'exports', 'process', 'Buffer', '__dirname',
  '__filename', 'setImmediate', 'clearImmediate'
]);

const browser = names([
  'window', 'document', 'navigator', 'location', 'localStorage',
  'indexedDB', 'requestAnimationFrame', 'cancelAnimationFrame',
  'EventSource', 'Event', 'FileReader', 'FormData', 'Blob', 'File',
  // The browser module the Face tab and Anymote load before their own script.
  'GlassFace'
]);

const rules = { 'no-undef': 'error' };
const linterOptions = { reportUnusedDisableDirectives: 'off' };

module.exports = [
  {
    files: ['plugin/**/*.js'],
    ignores: ['plugin/manager/face-page.js', '**/*.check.js'],
    linterOptions,
    languageOptions: { ecmaVersion: 2022, sourceType: 'commonjs', globals: { ...shared, ...node } },
    rules
  },
  {
    files: ['plugin/manager/face-page.js', '**/*.check.js'],
    linterOptions,
    languageOptions: { ecmaVersion: 2022, sourceType: 'script', globals: { ...shared, ...browser } },
    rules
  }
];
