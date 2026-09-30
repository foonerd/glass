// The Face tab: the display's pipeline in the browser. The module is the
// same code the player draws with, compiled to WebAssembly; this script is
// the page's side of its contract (bins/glass-face/src/lib.rs): fetch what
// the module asks for, put it in, feed it the frames and the plugin's
// lines from the manager's event stream, and blit a frame at the theme's
// rate. Files are kept in the browser by checksum, so a second visit
// fetches only what changed on the player.
(function () {
  'use strict';

  var face = {
    t: function (key) { return key; },
    version: function () { return ''; },
    shown: false,
    ex: null,
    ready: false,
    theme: '',
    meter: '',
    wanted: '',
    rate: 30,
    width: 0,
    height: 0,
    ctx: null,
    image: null,
    stream: null,
    frameAt: 0,
    loop: null,
    frames: false,
    starting: null,
    restart: false,
    // Anymote: the canvas sized to the window, keeping the theme's shape.
    fit: false,
    // The plugin's lines that came before the module was up, replayed to it.
    earlyLines: [],
    // The meter the player's own display shows, from its showing lines.
    playerMeter: '',
    // On a page of its own the first tap takes the screen, unless the
    // screen was given back on purpose.
    leftFullscreen: false
  };

  var $ = function (id) { return document.getElementById(id); };
  var encoder = new TextEncoder();
  var decoder = new TextDecoder();

  function say(text, problem) {
    var el = $('face-state');
    if (!el) return;
    el.textContent = text || '';
    el.style.color = problem ? 'var(--danger)' : '';
  }

  // ---- the module's memory ----------------------------------------------
  function memory() { return new Uint8Array(face.ex.memory.buffer); }
  function putBytes(bytes) {
    var ptr = face.ex.alloc(bytes.length);
    memory().set(bytes, ptr);
    return ptr;
  }
  function withBytes(bytes, fn) {
    var ptr = putBytes(bytes);
    try { return fn(ptr, bytes.length); } finally { face.ex.free(ptr, bytes.length); }
  }
  function withString(text, fn) { return withBytes(encoder.encode(text), fn); }
  function answer() {
    var ptr = face.ex.answer_ptr(), len = face.ex.answer_len();
    return decoder.decode(memory().subarray(ptr, ptr + len));
  }
  // A trap in the module (a panic) leaves its reason as the answer.
  function guarded(fn) {
    try { return fn(); } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) {
        var why = '';
        try { why = answer(); } catch (e2) { /* the memory is gone */ }
        face.ready = false;
        throw new Error(why || e.message);
      }
      throw e;
    }
  }
  // A call that answers: the code and the answer text.
  function ask(name, text) {
    return guarded(function () {
      return withString(text, function (ptr, len) {
        var code = face.ex[name](ptr, len);
        return { code: code, answer: answer() };
      });
    });
  }

  // ---- files kept by checksum --------------------------------------------
  var db = null;
  function openDb() {
    if (db) return Promise.resolve(db);
    return new Promise(function (resolve) {
      var request;
      try { request = indexedDB.open('glass-face', 1); } catch (e) { return resolve(null); }
      request.onupgradeneeded = function () { request.result.createObjectStore('files'); };
      request.onsuccess = function () { db = request.result; resolve(db); };
      request.onerror = function () { resolve(null); };
      request.onblocked = function () { resolve(null); };
    });
  }
  function cached(sha) {
    return openDb().then(function (db) {
      if (!db || !sha) return null;
      return new Promise(function (resolve) {
        var tx = db.transaction('files', 'readonly').objectStore('files').get(sha);
        tx.onsuccess = function () { resolve(tx.result ? new Uint8Array(tx.result) : null); };
        tx.onerror = function () { resolve(null); };
      });
    });
  }
  function keep(sha, bytes) {
    return openDb().then(function (db) {
      if (!db || !sha) return;
      try { db.transaction('files', 'readwrite').objectStore('files').put(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), sha); } catch (e) { /* the browser refused: fetched next time */ }
    });
  }

  // Every file of a plan into the module, from the cache or the manager.
  function bring(plan) {
    var fetched = 0, kept = 0;
    var next = function (i) {
      if (i >= plan.length) return Promise.resolve({ fetched: fetched, kept: kept });
      var item = plan[i];
      return cached(item.sha256).then(function (bytes) {
        if (bytes) { kept++; return bytes; }
        return fetch(item.url).then(function (res) {
          if (!res.ok) throw new Error(item.path + ': ' + res.status);
          return res.arrayBuffer();
        }).then(function (buffer) {
          var got = new Uint8Array(buffer);
          fetched++;
          keep(item.sha256, got);
          return got;
        });
      }).then(function (bytes) {
        guarded(function () {
          withString(item.path, function (pp, pl) {
            withBytes(bytes, function (dp, dl) { face.ex.put_file(pp, pl, dp, dl); });
          });
        });
        if ((i & 7) === 7) say(face.t('MANAGER_FACE_LOADING') + ' ' + (i + 1) + '/' + plan.length);
        return next(i + 1);
      });
    };
    return next(0);
  }

  // ---- the module and the theme -------------------------------------------
  function loadModule() {
    if (face.ex) return Promise.resolve();
    return fetch('/face/glass-face.wasm?v=' + encodeURIComponent(face.version())).then(function (res) {
      if (res.status === 404) throw new Error(face.t('MANAGER_FACE_NO_MODULE'));
      if (!res.ok) throw new Error('module: ' + res.status);
      return res.arrayBuffer();
    }).then(function (bytes) {
      return WebAssembly.instantiate(bytes, {});
    }).then(function (result) {
      face.ex = result.instance.exports;
      var kept = face.earlyLines;
      face.earlyLines = [];
      kept.forEach(function (line) {
        try { guarded(function () { withString(line, function (ptr, len) { face.ex.event(ptr, len); }); }); } catch (err) { /* told at start */ }
      });
    });
  }

  // The theme on show, as a remote brings it: the configuration, the fonts
  // and icons, the theme's files; then the meter on show.
  function bringTheme() {
    say(face.t('MANAGER_FACE_LOADING'));
    face.ready = false;
    var config;
    return fetch('/api/remote/config').then(function (res) { return res.json(); }).then(function (data) {
      config = data;
      var configured = ask('configure', JSON.stringify(config));
      if (configured.code) throw new Error(configured.answer);
      face.theme = config.theme;
      var assets = JSON.parse(configured.answer);
      return fetch('/api/themes/' + encodeURIComponent(config.theme) + '/files').then(function (res) {
        if (!res.ok) throw new Error('theme files: ' + res.status);
        return res.json();
      }).then(function (files) {
        var planned = ask('theme_files', JSON.stringify(files));
        if (planned.code) throw new Error(planned.answer);
        return bring(assets.concat(JSON.parse(planned.answer)));
      });
    }).then(function () {
      start(face.wanted || face.playerMeter || config.meter || '');
    });
  }

  // Put a meter on show: the configuration's own for an empty name.
  function start(meter) {
    var started = ask('start', meter === 'random' ? '' : meter);
    if (started.code) throw new Error(started.answer);
    face.meter = started.answer;
    sized();
  }

  // The canvas at the meter's size, and the frames may flow.
  function sized() {
    face.rate = face.ex.frame_rate() || 30;
    face.width = face.ex.frame_width();
    face.height = face.ex.frame_height();
    var canvas = $('face-canvas');
    canvas.width = face.width;
    canvas.height = face.height;
    face.ctx = canvas.getContext('2d');
    face.image = face.ctx.createImageData(face.width, face.height);
    face.ready = true;
    listen(canvas);
    fitted();
    showing();
  }

  // On a page of its own the canvas fills the window, the theme's shape kept.
  function fitted() {
    if (!face.fit || !face.width || !face.height) return;
    var canvas = $('face-canvas');
    var scale = Math.min(window.innerWidth / face.width, window.innerHeight / face.height);
    canvas.style.width = Math.max(1, Math.floor(face.width * scale)) + 'px';
    canvas.style.height = Math.max(1, Math.floor(face.height * scale)) + 'px';
  }

  // ---- the finger on the controls ---------------------------------------
  // A pointer event goes to the module in the frame's pixels; what comes
  // back is done here: a command to the player through the manager, a
  // meter stepped, or a dismiss, which leaves full screen.
  var listening = false;
  function listen(canvas) {
    if (listening) return;
    listening = true;
    canvas.style.touchAction = 'none';
    var send = function (kind, e) {
      if (!face.ready || !face.ex || typeof face.ex.pointer !== 'function') return;
      var rect = canvas.getBoundingClientRect();
      if (!rect.width || !rect.height) return;
      var x = Math.round((e.clientX - rect.left) * face.width / rect.width);
      var y = Math.round((e.clientY - rect.top) * face.height / rect.height);
      var acts;
      try {
        acts = guarded(function () { face.ex.pointer(kind, x, y); return JSON.parse(answer() || '[]'); });
      } catch (err) { say(face.t('MANAGER_FACE_FAILED') + ' ' + err.message, true); return; }
      acts.forEach(function (act) {
        if (act.command) {
          fetch('/api/face/command', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(act.command) }).catch(function () {});
        }
        if (act.meter) { face.meter = act.meter; sized(); }
        if (act.dismiss && document.fullscreenElement) document.exitFullscreen();
      });
    };
    canvas.addEventListener('pointerdown', function (e) {
      if (e.pointerType === 'mouse' && e.button !== 0) return;
      try { canvas.setPointerCapture(e.pointerId); } catch (err) { /* not for us */ }
      send(0, e);
      e.preventDefault();
    });
    canvas.addEventListener('pointermove', function (e) {
      if (e.pointerType === 'mouse' && e.buttons === 0) return;
      send(1, e);
    });
    canvas.addEventListener('pointerup', function (e) {
      send(2, e);
      if (face.fit && !document.fullscreenElement && !face.leftFullscreen) fullScreen();
    });
    canvas.addEventListener('pointercancel', function (e) { send(2, e); });
  }

  function showing() {
    var text = face.theme + ' · ' + face.meter + ' · ' + face.width + '×' + face.height + ' · ' + face.rate + ' fps';
    if (!face.frames) text += ' · ' + face.t('MANAGER_FACE_NO_FRAMES');
    say(text);
  }

  // ---- the truth after the period ----------------------------------------
  // The player's own display leaves the screen when the persist period ends
  // after a stop or a pause; a page stays, and says so instead of standing
  // still: a banner over the picture until the player plays again.
  function truth() {
    var banner = $('face-truth');
    if (!banner) {
      banner = document.createElement('div');
      banner.id = 'face-truth';
      banner.style.cssText = 'position:absolute;left:0;right:0;bottom:12%;text-align:center;padding:10px 16px;font:600 clamp(14px,2.2vw,22px) sans-serif;color:#fff;background:rgba(0,0,0,.55);display:none;pointer-events:none;';
      var canvas = $('face-canvas');
      if (canvas && canvas.parentNode) { canvas.parentNode.style.position = canvas.parentNode.style.position || 'relative'; canvas.parentNode.appendChild(banner); }
    }
    if (face.truthTimer) { clearTimeout(face.truthTimer); face.truthTimer = null; }
    var stopped = face.status && face.status !== 'play';
    var persist = face.persist || {};
    var left = stopped && persist.startedAt ? persist.seconds * 1000 - (Date.now() - persist.startedAt) : 0;
    if (stopped && (!persist.startedAt || left <= 0)) {
      banner.textContent = face.t(face.status === 'pause' ? 'MANAGER_FACE_PAUSED' : 'MANAGER_FACE_STOPPED');
      banner.style.display = 'block';
    } else {
      banner.style.display = 'none';
      if (stopped && left > 0) face.truthTimer = setTimeout(truth, left + 200);
    }
  }

  // ---- the manager's stream: frames and the plugin's lines -----------------
  function connect() {
    if (face.stream) return;
    var stream = new EventSource('/api/face/events');
    face.stream = stream;
    stream.addEventListener('hop', function (e) {
      if (!face.ex || !face.ready) return;
      var text = atob(e.data);
      var bytes = new Uint8Array(text.length);
      for (var i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
      try { guarded(function () { withBytes(bytes, function (ptr, len) { face.ex.hop(ptr, len); }); }); } catch (err) { say(face.t('MANAGER_FACE_FAILED') + ' ' + err.message, true); }
    });
    stream.addEventListener('plugin', function (e) {
      var line = e.data;
      var message = null;
      try { message = JSON.parse(line); } catch (err) { return; }
      if (message && message.kind === 'showing' && message.meter) face.playerMeter = message.meter;
      if (message && message.kind === 'state' && message.state) { face.status = String(message.state.status || ''); truth(); }
      if (message && message.kind === 'persist') { face.persist = { mode: message.mode || '', seconds: message.seconds || 0, startedAt: message.startedAt || 0 }; truth(); }
      // Before the module is up the lines are kept for it: the feed
      // replays the player's state at once, ahead of the module's load.
      if (!face.ex) { face.earlyLines.push(line); return; }
      try { guarded(function () { withString(line, function (ptr, len) { face.ex.event(ptr, len); }); }); } catch (err) { say(face.t('MANAGER_FACE_FAILED') + ' ' + err.message, true); }
      if (!message) return;
      // The theme changed on the player: bring it again. The player's own
      // display moved to another meter: follow it.
      if (message.kind === 'config' && face.ready && message.theme && message.theme !== face.theme) {
        face.wanted = '';
        restart();
      } else if (message.kind === 'showing' && face.ready && message.meter && message.meter !== face.meter && !face.wanted) {
        try { start(message.meter); } catch (err) { say(String(err.message || err), true); }
      }
    });
    stream.addEventListener('feed', function (e) {
      try { face.frames = !!JSON.parse(e.data).frames; } catch (err) { /* not for us */ }
      if (face.ready) showing();
    });
    stream.onerror = function () {
      if (face.ready) say(face.t('MANAGER_FACE_WAITING'));
    };
  }

  function disconnect() {
    if (face.stream) { face.stream.close(); face.stream = null; }
  }

  function restart() {
    if (face.starting) { face.restart = true; return; }
    face.starting = bringTheme().catch(function (err) {
      say(face.t('MANAGER_FACE_FAILED') + ' ' + String(err.message || err), true);
    }).then(function () {
      face.starting = null;
      if (face.restart) { face.restart = false; restart(); }
    });
  }

  // ---- frames at the theme's rate -----------------------------------------
  function tick(now) {
    if (!face.shown) return;
    face.loop = requestAnimationFrame(tick);
    if (!face.ready || document.hidden) return;
    var period = 1000 / face.rate;
    if (now - face.frameAt < period * 0.9) return;
    face.frameAt = now;
    var ptr;
    // The engine's clock is the wall clock, so the theme's clocks and the persist countdown are true.
  try { ptr = guarded(function () { return face.ex.frame(BigInt(Date.now())); }); } catch (err) { say(face.t('MANAGER_FACE_FAILED') + ' ' + err.message, true); return; }
    if (!ptr) return;
    var size = face.width * face.height * 4;
    face.image.data.set(memory().subarray(ptr, ptr + size));
    face.ctx.putImageData(face.image, 0, 0);
    serve(now);
  }

  // ---- what the meter wants ----------------------------------------------
  // Pictures the module cannot fetch for itself: the album art, the fanart
  // and the pictures of the track's folder. Each want is fetched through
  // the manager once: a file lands with put_file, one the manager has not
  // got is marked missing, and a fanart set is answered as the manager's
  // JSON. A fetch that fails is tried again after a while; the module
  // keeps listing what it still wants.
  var wanting = {};
  var wantedAt = 0;
  function serve(now) {
    if (!face.ex || typeof face.ex.wants !== 'function' || now - wantedAt < 250) return;
    wantedAt = now;
    var list;
    try { list = guarded(function () { face.ex.wants(); return JSON.parse(answer() || '[]'); }); } catch (e) { return; }
    list.forEach(function (want) {
      var key = want.kind === 'file' ? 'file:' + want.path : 'fanart:' + want.artist + '\n' + want.uri;
      if (wanting[key]) return;
      wanting[key] = true;
      var done = function () { delete wanting[key]; };
      var later = function () { setTimeout(done, 10000); };
      if (want.kind === 'file') {
        fetch(want.url).then(function (res) {
          if (res.status === 404) {
            guarded(function () { withString(want.path, function (p, l) { face.ex.missing(p, l); }); });
            done();
            return null;
          }
          if (!res.ok) throw new Error(String(res.status));
          return res.arrayBuffer().then(function (buffer) {
            var bytes = new Uint8Array(buffer);
            guarded(function () {
              withString(want.path, function (pp, pl) {
                withBytes(bytes, function (dp, dl) { face.ex.put_file(pp, pl, dp, dl); });
              });
            });
            done();
          });
        }).catch(later);
      } else if (want.kind === 'fanart') {
        fetch('/api/face/fanart?artist=' + encodeURIComponent(want.artist) + '&uri=' + encodeURIComponent(want.uri)).then(function (res) {
          if (!res.ok) throw new Error(String(res.status));
          return res.text();
        }).catch(function () {
          // No answer is an empty set: the meter asks again with the next track.
          return '{"success":false}';
        }).then(function (text) {
          guarded(function () { withString(text, function (p, l) { face.ex.fanart_answer(p, l); }); });
          done();
        });
      } else {
        done();
      }
    });
  }

  // ---- the tab -------------------------------------------------------------
  function show() {
    if (face.shown) return;
    face.shown = true;
    connect();
    face.loop = requestAnimationFrame(tick);
    if (face.ready) { showing(); return; }
    face.starting = loadModule().then(bringTheme).catch(function (err) {
      say(face.t('MANAGER_FACE_FAILED') + ' ' + String(err.message || err), true);
    }).then(function () {
      face.starting = null;
      if (face.restart) { face.restart = false; restart(); }
    });
  }

  function hide() {
    if (!face.shown) return;
    face.shown = false;
    if (face.loop) { cancelAnimationFrame(face.loop); face.loop = null; }
    disconnect();
  }

  function fullScreen() {
    var stage = $('face-stage');
    if (!stage) return;
    if (document.fullscreenElement) { document.exitFullscreen(); return; }
    if (stage.requestFullscreen) stage.requestFullscreen().catch(function () {});
  }

  window.GlassFace = {
    init: function (options) {
      face.t = options.t || face.t;
      face.version = options.version || face.version;
      face.fit = !!options.fit;
      var button = $('btn-face-full');
      if (button) button.addEventListener('click', fullScreen);
      if (face.fit) {
        window.addEventListener('resize', fitted);
        document.addEventListener('fullscreenchange', function () {
          fitted();
          if (!document.fullscreenElement && face.wasFullscreen) face.leftFullscreen = true;
          face.wasFullscreen = !!document.fullscreenElement;
        });
      }
    },
    show: show,
    hide: hide
  };
})();
