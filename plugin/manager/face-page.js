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

  // The movie: what the player says of it (the file to play, the boxes of
  // the theme's meters, the sound's delay), the video element laid over
  // the theme's box, and the player's position it is held to.
  var movie = { line: null, video: null, src: '', stateAt: 0, timer: null, failed: '', hole: '' };

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
  // The browser's own store keeps every file brought, by its checksum, so
  // a page opened again brings nothing twice. The store is a convenience
  // and never a dependency: a store that does not answer within a moment,
  // refuses, or is full is passed over and the file fetched from the
  // player, and a full store is emptied so it can serve again.
  var db = null;
  var STORE_WAIT_MS = 1500;
  function withinAMoment(promise) {
    return Promise.race([promise, new Promise(function (resolve) { setTimeout(function () { resolve(null); }, STORE_WAIT_MS); })]);
  }
  function openDb() {
    if (db) return Promise.resolve(db);
    return withinAMoment(new Promise(function (resolve) {
      var request;
      try { request = indexedDB.open('glass-face', 1); } catch (e) { return resolve(null); }
      request.onupgradeneeded = function () { request.result.createObjectStore('files'); };
      request.onsuccess = function () { db = request.result; db.onversionchange = function () { db.close(); db = null; }; resolve(db); };
      request.onerror = function () { resolve(null); };
      request.onblocked = function () { resolve(null); };
    })).catch(function () { return null; });
  }
  function cached(sha) {
    return openDb().then(function (db) {
      if (!db || !sha) return null;
      return withinAMoment(new Promise(function (resolve) {
        var tx;
        try { tx = db.transaction('files', 'readonly').objectStore('files').get(sha); } catch (e) { return resolve(null); }
        tx.onsuccess = function () { resolve(tx.result ? new Uint8Array(tx.result) : null); };
        tx.onerror = function () { resolve(null); };
      }));
    }).catch(function () { return null; });
  }
  function keep(sha, bytes) {
    return openDb().then(function (db) {
      if (!db || !sha) return;
      var data = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      try {
        var store = db.transaction('files', 'readwrite').objectStore('files');
        var put = store.put(data, sha);
        put.onerror = function () {
          // A full store is emptied, so the files brought from now on fit again.
          if (put.error && put.error.name === 'QuotaExceededError') { try { db.transaction('files', 'readwrite').objectStore('files').clear(); } catch (e) { /* left as it is */ } }
        };
      } catch (e) { /* the browser refused: fetched next time */ }
    }).catch(function () { /* the store is no dependency */ });
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
  // The Manager says which module a page brings: Glass's own, the theme
  // alone, or the one the glass-evo component carries, the same with the
  // face over the theme, where the user's choice of what the views show
  // and the player's screen call for it.
  function moduleInfo() {
    return fetch('/api/face/module').then(function (res) {
      if (!res.ok) throw new Error('module: ' + res.status);
      return res.json();
    }).catch(function () {
      return { module: 'glass', url: '/face/glass-face.wasm?v=' + encodeURIComponent(face.version()), theme: null };
    });
  }
  function loadModule() {
    if (face.ex) return Promise.resolve();
    return moduleInfo().then(function (info) {
      face.module = info.module;
      return fetch(info.url);
    }).then(function (res) {
      if (res.status === 404) throw new Error(face.t('MANAGER_FACE_NO_MODULE'));
      if (!res.ok) throw new Error('module: ' + res.status);
      return res.arrayBuffer();
    }).then(function (bytes) {
      return WebAssembly.instantiate(bytes, {});
    }).then(function (result) {
      face.ex = result.instance.exports;
      face.zoneAt = undefined;
      var kept = face.earlyLines;
      face.earlyLines = [];
      kept.forEach(function (line) {
        try { guarded(function () { withString(line, function (ptr, len) { face.ex.event(ptr, len); }); }); } catch (err) { /* told at start */ }
      });
      // What this page does with the sound, for the sheet's tile, from the start.
      phTell();
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
    }).then(bringFaceTheme).then(function () {
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
    movieApply();
    // The banner was settled when the player's lines came, before the
    // module was here to say whether it draws a face: settled again now.
    truth();
  }

  // On a page of its own the canvas fills the window, the theme's shape kept.
  function fitted() {
    if (!face.fit || !face.width || !face.height) return;
    var canvas = $('face-canvas');
    var scale = Math.min(window.innerWidth / face.width, window.innerHeight / face.height);
    canvas.style.width = Math.max(1, Math.floor(face.width * scale)) + 'px';
    canvas.style.height = Math.max(1, Math.floor(face.height * scale)) + 'px';
    moviePlace();
  }

  // ---- the movie in the browser ------------------------------------------
  // The browser plays the file itself, muted, in the theme's box over the
  // canvas, and holds it to the player's position less the sound's delay
  // on the player (the display measured it) plus the lead: the picture
  // meets the sound in the room. A file the browser cannot play (MKV, AVI)
  // gets a word, not a hole.
  function moviePosition() {
    var s = face.playerState;
    if (!s || !movie.stateAt) return null;
    var seek = Number(s.seek || 0) / 1000;
    if (s.status === 'play') return seek + (Date.now() - movie.stateAt) / 1000;
    if (s.status === 'pause') return seek;
    return null;
  }
  function movieElement() {
    if (movie.video) return movie.video;
    var stage = $('face-stage');
    var canvas = $('face-canvas');
    if (!stage || !canvas) return null;
    var v = document.createElement('video');
    v.id = 'face-movie';
    v.muted = true;
    v.defaultMuted = true;
    v.playsInline = true;
    v.setAttribute('playsinline', '');
    v.preload = 'auto';
    v.style.position = 'absolute';
    v.style.pointerEvents = 'none';
    v.style.display = 'none';
    v.addEventListener('error', function () {
      movie.failed = movie.src;
      v.style.display = 'none';
      say(face.t('MANAGER_FACE_MOVIE_UNPLAYABLE'), true);
    });
    v.addEventListener('loadedmetadata', function () { movieFollow(true); });
    ['loadeddata', 'playing', 'emptied', 'ended', 'stalled'].forEach(function (ev) { v.addEventListener(ev, function () { moviePlace(); }); });
    if (window.getComputedStyle(stage).position === 'static') stage.style.position = 'relative';
    stage.appendChild(v);
    movie.video = v;
    return v;
  }
  // The element placed in the box of the meter on show, as the canvas is
  // drawn on the page, always under the canvas: for an overlay box the
  // module leaves the theme clear there while a picture shows, so the
  // picture shows through the theme and the face stays over it; for a
  // background box the theme's own clear parts let it through.
  function moviePlace() {
    var v = movie.video;
    if (!v || !movie.line || !face.ready) return;
    var box = movie.line.boxes ? movie.line.boxes[face.meter] : null;
    if (!box || !movie.src || movie.failed) { v.style.display = 'none'; movieHole(null); return; }
    var canvas = $('face-canvas');
    var rect = canvas.getBoundingClientRect();
    var stageRect = $('face-stage').getBoundingClientRect();
    var k = face.width ? rect.width / face.width : 1;
    v.style.left = (rect.left - stageRect.left + box.x * k) + 'px';
    v.style.top = (rect.top - stageRect.top + box.y * k) + 'px';
    v.style.width = (box.w * k) + 'px';
    v.style.height = (box.h * k) + 'px';
    v.style.objectFit = box.scale === 'cover' ? 'cover' : (box.scale === 'stretch' ? 'fill' : 'contain');
    v.style.zIndex = '0';
    canvas.style.position = 'relative';
    canvas.style.zIndex = '1';
    v.style.display = '';
    movieHole(box.zorder === 'background' || v.readyState < 2 ? null : box);
  }
  // The theme's box cut clear in the module, or painted whole again.
  function movieHole(box) {
    if (!face.ex || typeof face.ex.set_hole !== 'function') return;
    var key = box ? [box.x, box.y, box.w, box.h].join(',') : '';
    if (movie.hole === key) return;
    movie.hole = key;
    try {
      guarded(function () { if (box) face.ex.set_hole(box.x, box.y, box.w, box.h); else face.ex.clear_hole(); });
    } catch (e) { /* an older module: the picture sits under an opaque theme */ }
  }
  // The player's word applied: the file to play, or none.
  function movieApply() {
    var line = movie.line;
    var v = movieElement();
    if (!v) return;
    var wanted = line && line.on && line.url ? line.url : '';
    if (line && line.on && line.file && line.playable === false) {
      say(face.t('MANAGER_FACE_MOVIE_UNPLAYABLE'), true);
    } else if (movie.src && !wanted && !movie.failed) {
      say('');
    }
    if (wanted !== movie.src) {
      movie.src = wanted;
      movie.failed = '';
      if (wanted) { v.setAttribute('src', wanted); v.load(); } else { v.removeAttribute('src'); try { v.load(); } catch (e) { /* nothing loaded */ } }
    }
    moviePlace();
    movieFollow(true);
    if (wanted && !movie.timer) movie.timer = setInterval(function () { movieFollow(false); }, 500);
    if (!wanted && movie.timer) { clearInterval(movie.timer); movie.timer = null; }
  }
  // The element held to the player's position: a jump seeks it, a drift
  // nudges its rate, play and pause follow the player.
  function movieFollow(jump) {
    var v = movie.video;
    if (!v || !movie.src || !movie.line || movie.failed) return;
    var position = moviePosition();
    if (position === null) { if (!v.paused) v.pause(); return; }
    var target = position - Number(movie.line.delay || 0) / 1000 + Number(movie.line.lead || 0) / 1000;
    if (target < 0) target = 0;
    var playing = face.playerState && face.playerState.status === 'play';
    if (!isFinite(v.duration)) return;
    var diff = v.currentTime - target;
    if (jump || Math.abs(diff) > 0.25) {
      try { v.currentTime = target; } catch (e) { /* not seekable yet */ }
      v.playbackRate = 1;
    } else if (Math.abs(diff) > 0.04) {
      v.playbackRate = diff > 0 ? 0.97 : 1.03;
    } else {
      v.playbackRate = 1;
    }
    if (playing && v.paused) { var p = v.play(); if (p && p.catch) p.catch(function () { /* the browser wants a gesture: the next tap */ }); }
    if (!playing && !v.paused) v.pause();
  }

  // ---- the finger on the controls ---------------------------------------
  // A pointer event goes to the module in the frame's pixels; what comes
  // back is done here: a command to the player through the manager, a
  // meter stepped, or a dismiss, which leaves full screen. A module that
  // carries a face may also ask at a frame (a finger resting on a button),
  // and that is taken after every frame and done the same way.
  function carry(acts) {
    acts.forEach(function (act) {
      // Play here is this page's own doing, never the player's: the More
      // sheet's tile and its slider ask here, and the face is told back.
      if (act.command && act.command.name === 'play-here') {
        var ask = act.command.value || {};
        if (typeof ask.gain === 'number') phGain(ask.gain);
        if (ask.on === true && !ph.wanted) phStart();
        else if (ask.on === false && ph.wanted) phStop();
        else phTell();
        return;
      }
      if (act.command) {
        // A command with an answer (a browse) brings a line of the
        // plugin's back, handed to the module as the feed's lines are.
        fetch('/api/face/command', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(act.command) })
          .then(function (r) { return r.json(); })
          .then(function (a) {
            if (!a || !a.line || !face.ex) return;
            var line = JSON.stringify(a.line);
            guarded(function () { withString(line, function (ptr, len) { face.ex.event(ptr, len); }); });
          })
          .catch(function () {});
      }
      if (act.meter) {
        face.meter = act.meter; sized(); moviePlace();
        // Until the screen has answered with this meter (or three seconds
        // passed) its older answers are not followed: a second press must
        // not be taken back by the answer to the first.
        face.asked = act.meter; face.askedAt = Date.now();
        // The player's screen shows it too; its answer brings the other views along.
        fetch('/api/face/meter', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ meter: act.meter }) }).catch(function () {});
      }
      if (act.dismiss && document.fullscreenElement) document.exitFullscreen();
    });
  }
  // A face tells the time of day: the module is told the page's zone, its
  // minutes east of universal time and its short name, at the start and
  // once a minute after, so a change of the clocks is followed.
  function zone(now) {
    if (typeof face.ex.zone !== 'function' || (face.zoneAt !== undefined && now - face.zoneAt < 60000)) return;
    face.zoneAt = now;
    var name = '';
    try {
      var part = new Intl.DateTimeFormat(undefined, { timeZoneName: 'short' }).formatToParts(new Date()).filter(function (p) { return p.type === 'timeZoneName'; })[0];
      name = part ? part.value : '';
    } catch (err) { /* no name: the offset alone */ }
    guarded(function () { withString(name, function (p, l) { face.ex.zone(-new Date().getTimezoneOffset(), p, l); }); });
  }
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
      carry(acts);
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
  // still: a banner over the picture until the player plays again. On a
  // screen that is the display's own (no kiosk, or glass-evo holding it)
  // the display stays too, and the banner says only that the player
  // stands still.
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
    // A module with a face over the theme shows the player standing still
    // itself, the clock on black as the player's screen has it: no banner.
    if (stopped && (!persist.startedAt || left <= 0) && !overlaid()) {
      banner.textContent = face.t((face.status === 'pause' ? 'MANAGER_FACE_PAUSED' : 'MANAGER_FACE_STOPPED') + (persist.stays ? '_STAYS' : ''));
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
      if (message && message.kind === 'state' && message.state) { face.status = String(message.state.status || ''); face.playerState = message.state; movie.stateAt = Date.now(); truth(); phSession(); movieFollow(false); }
      if (message && message.kind === 'movie') { movie.line = message; movieApply(); return; }
      if (message && message.kind === 'persist') { face.persist = { mode: message.mode || '', seconds: message.seconds || 0, startedAt: message.startedAt || 0, stays: !!message.stays }; truth(); }
      // What the views carry changed: the other module, if this is not it.
      if (message && message.kind === 'views') {
        if (face.ex && !!message.face !== (face.module === 'glass-evo')) swapModule();
        return;
      }
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
      } else if (message.kind === 'showing' && face.ready && message.meter) {
        if (face.asked && (message.meter === face.asked || Date.now() - face.askedAt > 3000)) face.asked = '';
        if (message.meter !== face.meter && !face.wanted && !face.asked) {
          try { start(message.meter); } catch (err) { say(String(err.message || err), true); }
        }
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

  // Whether the module on show draws a face over the theme.
  function overlaid() {
    return !!(face.ex && typeof face.ex.overlaid === 'function' && face.ex.overlaid());
  }
  // The face reads its theme where a player keeps the user's face themes:
  // the text of the one the settings name goes into the module's table
  // under `faces`, whichever folder the Manager found it in.
  function bringFaceTheme() {
    if (!overlaid()) return Promise.resolve();
    return moduleInfo().then(function (info) {
      if (!info.theme || !info.theme.name) return;
      var bytes = new TextEncoder().encode(String(info.theme.text || ''));
      guarded(function () {
        withString('faces/' + info.theme.name + '/face.txt', function (pp, pl) {
          withBytes(bytes, function (dp, dl) { face.ex.put_file(pp, pl, dp, dl); });
        });
      });
      // And the theme's own files, its skies, beside it, before the first frame.
      return Promise.all((info.theme.files || []).map(function (file) {
        return fetch(file.url).then(function (res) {
          if (!res.ok) throw new Error(file.path + ': ' + res.status);
          return res.arrayBuffer();
        }).then(function (buffer) {
          var data = new Uint8Array(buffer);
          guarded(function () {
            withString('faces/' + info.theme.name + '/' + file.path, function (pp, pl) {
              withBytes(data, function (dp, dl) { face.ex.put_file(pp, pl, dp, dl); });
            });
          });
        }).catch(function () { /* a sky that did not come is drawn */ });
      }));
    });
  }
  // The other module is called for (the screen changed hands, or the user
  // chose otherwise): this one is let go and the page starts over.
  function swapModule() {
    if (face.starting) { face.swap = true; return; }
    face.ex = null;
    face.ready = false;
    truth();
    face.starting = loadModule().then(bringTheme).catch(function (err) {
      say(face.t('MANAGER_FACE_FAILED') + ' ' + String(err.message || err), true);
      tryAgainLater();
    }).then(function () {
      face.starting = null;
      if (face.swap) { face.swap = false; swapModule(); } else if (face.restart) { face.restart = false; restart(); }
    });
  }

  function restart() {
    if (face.starting) { face.restart = true; return; }
    face.starting = bringTheme().catch(function (err) {
      say(face.t('MANAGER_FACE_FAILED') + ' ' + String(err.message || err), true);
      tryAgainLater();
    }).then(function () {
      face.starting = null;
      if (face.restart) { face.restart = false; restart(); }
    });
  }

  // A start that failed (a file the player could not give, a stream that
  // broke, a store that misbehaved) is tried again by itself while the tab
  // is shown, every few seconds, so a page never stays empty for want of
  // a refresh; the line says what failed until it comes up.
  var RETRY_MS = 5000;
  function tryAgainLater() {
    if (face.retry) return;
    face.retry = setTimeout(function () {
      face.retry = null;
      if (face.shown && !face.ready && !face.starting) restart();
    }, RETRY_MS);
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
    var asked = [];
    // The engine's clock is the wall clock, so the theme's clocks and the persist countdown are true.
    try {
      zone(now);
      ptr = guarded(function () { return face.ex.frame(BigInt(Date.now())); });
      if (typeof face.ex.taken === 'function') asked = guarded(function () { return face.ex.taken() ? JSON.parse(answer() || '[]') : []; });
    } catch (err) { say(face.t('MANAGER_FACE_FAILED') + ' ' + err.message, true); return; }
    carry(asked);
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
    if (face.ready) { showing(); movieApply(); return; }
    face.starting = loadModule().then(bringTheme).catch(function (err) {
      say(face.t('MANAGER_FACE_FAILED') + ' ' + String(err.message || err), true);
      tryAgainLater();
    }).then(function () {
      face.starting = null;
      if (face.restart) { face.restart = false; restart(); }
    });
  }

  function hide() {
    if (!face.shown) return;
    face.shown = false;
    if (face.loop) { cancelAnimationFrame(face.loop); face.loop = null; }
    if (face.retry) { clearTimeout(face.retry); face.retry = null; }
    disconnect();
    // The movie rests with the tab: no bytes for a page nobody sees.
    if (movie.timer) { clearInterval(movie.timer); movie.timer = null; }
    if (movie.video && !movie.video.paused) movie.video.pause();
    movieHole(null);
  }

  function fullScreen() {
    var stage = $('face-stage');
    if (!stage) return;
    if (document.fullscreenElement) { document.exitFullscreen(); return; }
    if (stage.requestFullscreen) stage.requestFullscreen().catch(function () {});
  }

  // ---- Play here: the player's sound on this page --------------------------
  // The lite's PCM over the Manager's WebSocket route (/anymote/listen),
  // played by Web Audio on the context's clock a quarter second ahead (the
  // jitter buffer), with the page's own gain (the stream is at the player's
  // full level, before its volume), routed through a media stream and an
  // audio element where the browser has them, so a phone keeps playing with
  // its screen locked and shows the track on its lock screen. The control
  // shows only where the player's switch is on. The wire is outcast's
  // contract; pcm.js decodes the frames.
  var PH_LEAD = 0.25;
  var ph = { available: false, wanted: false, ws: null, ctx: null, gain: null, dest: null, el: null, segment: null, at: 0, retry: null, backoff: 1000, state: '', rateRefused: 0, gainValue: 0.8 };

  function phSay(key, fill) {
    var text = key ? face.t(key) : '';
    if (fill) Object.keys(fill).forEach(function (k) { text = text.replace('{' + k + '}', fill[k]); });
    ph.state = key || '';
    say(text, key === 'MANAGER_PLAY_HERE_OFF' || key === 'MANAGER_PLAY_HERE_UNAVAILABLE' || key === 'MANAGER_PLAY_HERE_FULL' || key === 'MANAGER_PLAY_HERE_RATE');
    phTell();
  }

  // The face is told what this page does with the sound, so the More
  // sheet's tile wears it and its slider stands where the gain is: a line
  // made here for the module alone, in the feed's own shape.
  function phTell() {
    if (!face.ex || typeof face.ex.event !== 'function') return;
    var line = JSON.stringify({ kind: 'play-here', served: ph.available, on: ph.wanted, gain: Math.round(ph.gainValue * 100) });
    try { guarded(function () { withString(line, function (ptr, len) { face.ex.event(ptr, len); }); }); } catch (err) { /* the module is not up yet */ }
  }

  // The device's own volume, 0 to 100, from the slider here or the face's.
  function phGain(value) {
    ph.gainValue = Math.max(0, Math.min(1, Number(value) / 100));
    if (ph.gain) ph.gain.gain.value = ph.gainValue;
    var g = $('play-here-gain');
    if (g) g.value = String(Math.round(ph.gainValue * 100));
    try { localStorage.setItem('glass-play-here-gain', String(Math.round(ph.gainValue * 100))); } catch (e) { /* no storage */ }
    phTell();
  }

  function phButton() {
    var b = $('btn-play-here');
    if (!b) return;
    b.classList.toggle('hidden', !ph.available);
    b.textContent = face.t(ph.wanted ? 'MANAGER_PLAY_HERE_STOP' : 'MANAGER_PLAY_HERE');
    var g = $('play-here-gain');
    if (g) { g.classList.toggle('hidden', !(ph.available && ph.wanted)); g.setAttribute('aria-label', face.t('MANAGER_PLAY_HERE_GAIN')); }
  }

  // Whether the player serves its sound now: the switch on and the lite there.
  function phAsk() {
    return fetch('/api/play-here').then(function (r) { return r.ok ? r.json() : null; }).then(function (p) {
      ph.available = !!(p && p.on && p.available);
      phButton();
      phTell();
    }).catch(function () { ph.available = false; phButton(); phTell(); });
  }

  function phStart() {
    ph.wanted = true;
    phButton();
    if (!ph.ctx) {
      var AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) { ph.wanted = false; phButton(); phSay('MANAGER_PLAY_HERE_UNAVAILABLE'); return; }
      ph.ctx = new AC();
      ph.gain = ph.ctx.createGain();
      ph.gain.gain.value = ph.gainValue;
      try {
        ph.dest = ph.ctx.createMediaStreamDestination();
        ph.gain.connect(ph.dest);
        ph.el = new Audio();
        ph.el.srcObject = ph.dest.stream;
      } catch (e) { ph.dest = null; ph.el = null; ph.gain.connect(ph.ctx.destination); }
    }
    // Inside the tap that asked: iOS resumes a context only then.
    if (ph.ctx.state === 'suspended') ph.ctx.resume().catch(function () {});
    if (ph.el) ph.el.play().catch(function () {});
    phSession();
    phConnect();
  }

  function phConnect() {
    if (!ph.wanted || ph.ws) return;
    if (ph.retry) { clearTimeout(ph.retry); ph.retry = null; }
    var url = (location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/anymote/listen';
    var ws;
    try { ws = new WebSocket(url); } catch (e) { phSay('MANAGER_PLAY_HERE_UNAVAILABLE'); return; }
    ws.binaryType = 'arraybuffer';
    ph.ws = ws;
    ph.segment = null; ph.at = 0;
    phSay('MANAGER_PLAY_HERE_CONNECTING');
    ws.onopen = function () { ph.backoff = 1000; if (!ph.segment) phSay('MANAGER_PLAY_HERE_WAITING'); };
    ws.onmessage = function (e) {
      if (typeof e.data === 'string') { phText(e.data); return; }
      phPcm(e.data);
    };
    ws.onerror = function () { /* the close follows */ };
    ws.onclose = function () {
      if (ph.ws !== ws) return;
      ph.ws = null; ph.segment = null; ph.at = 0;
      if (!ph.wanted) return;
      var full = ph.state === 'MANAGER_PLAY_HERE_FULL';
      // The player's word before trying again: the switch gone off is the
      // end of it; anything else is tried again with a backoff of a second
      // or two (the lite leaves when nobody listens and comes back on the
      // next ask).
      phAsk().then(function () {
        if (!ph.wanted) return;
        if (!ph.available) { ph.wanted = false; phButton(); phSay('MANAGER_PLAY_HERE_OFF'); return; }
        phSay(full ? 'MANAGER_PLAY_HERE_FULL' : 'MANAGER_PLAY_HERE_CONNECTING');
        ph.retry = setTimeout(phConnect, ph.backoff);
        ph.backoff = Math.min(ph.backoff * 2, 4000);
      });
    };
  }

  function phText(text) {
    var m = null;
    try { m = JSON.parse(text); } catch (e) { return; }
    if (!m || typeof m.type !== 'string') return;
    if (m.type === 'segment') {
      ph.segment = m; ph.at = 0; ph.rateRefused = 0;
      phSay(m.one_bit ? 'MANAGER_PLAY_HERE_DSD' : 'MANAGER_PLAY_HERE_PLAYING');
    } else if (m.type === 'end') {
      ph.segment = null; ph.at = 0;
      phSay('MANAGER_PLAY_HERE_WAITING');
    } else if (m.type === 'gap') {
      ph.at = 0;
    } else if (m.type === 'full') {
      phSay('MANAGER_PLAY_HERE_FULL');
    }
  }

  function phPcm(buffer) {
    var seg = ph.segment;
    if (!seg || !ph.ctx || !window.GlassPcm) return;
    var channels = window.GlassPcm.decode(seg, buffer);
    if (!channels || !channels[0].length) return;
    var buf;
    try { buf = ph.ctx.createBuffer(channels.length, channels[0].length, seg.rate); } catch (e) {
      // A rate this browser cannot take: said once per segment; the cap on
      // the player's System tab brings the stream down.
      if (ph.rateRefused !== seg.rate) { ph.rateRefused = seg.rate; phSay('MANAGER_PLAY_HERE_RATE', { rate: seg.rate }); }
      return;
    }
    for (var c = 0; c < channels.length; c++) buf.getChannelData(c).set(channels[c]);
    var src = ph.ctx.createBufferSource();
    src.buffer = buf;
    src.connect(ph.gain);
    var now = ph.ctx.currentTime;
    if (ph.at < now + 0.02) ph.at = now + PH_LEAD;
    src.start(ph.at);
    ph.at += buf.duration;
  }

  function phStop() {
    ph.wanted = false;
    if (ph.retry) { clearTimeout(ph.retry); ph.retry = null; }
    if (ph.ws) { var ws = ph.ws; ph.ws = null; try { ws.close(); } catch (e) { /* closed already */ } }
    ph.segment = null; ph.at = 0;
    if (ph.ctx && ph.ctx.state === 'running') ph.ctx.suspend().catch(function () {});
    if (ph.el) { try { ph.el.pause(); } catch (e) { /* not playing */ } }
    if ('mediaSession' in navigator) { try { navigator.mediaSession.playbackState = 'none'; } catch (e) { /* no session */ } }
    phSay('');
    phButton();
  }

  // The lock screen's words: the player's state as the feed brings it.
  function phSession() {
    if (!ph.wanted || !('mediaSession' in navigator)) return;
    var s = face.playerState || {};
    var art = String(s.albumart || '');
    if (art && art.charAt(0) === '/') art = location.protocol + '//' + location.hostname + ':3000' + art;
    try {
      navigator.mediaSession.metadata = new MediaMetadata({ title: s.title || 'Volumio', artist: s.artist || '', album: s.album || '', artwork: art ? [{ src: art }] : [] });
      navigator.mediaSession.playbackState = 'playing';
      navigator.mediaSession.setActionHandler('pause', phStop);
      navigator.mediaSession.setActionHandler('stop', phStop);
    } catch (e) { /* no session: the sound plays anyway */ }
  }

  function phInit() {
    var button = $('btn-play-here');
    if (!button) return;
    button.addEventListener('click', function () { if (ph.wanted) phStop(); else phStart(); });
    var g = $('play-here-gain');
    if (g) {
      try {
        var kept = localStorage.getItem('glass-play-here-gain');
        if (kept !== null && !isNaN(parseInt(kept, 10))) { ph.gainValue = Math.max(0, Math.min(1, parseInt(kept, 10) / 100)); g.value = String(Math.round(ph.gainValue * 100)); }
      } catch (e) { /* no storage: the default */ }
      g.addEventListener('input', function () { phGain(parseInt(g.value, 10)); });
    }
    phAsk();
  }

  window.GlassFace = {
    init: function (options) {
      face.t = options.t || face.t;
      face.version = options.version || face.version;
      face.fit = !!options.fit;
      var button = $('btn-face-full');
      if (button) button.addEventListener('click', fullScreen);
      phInit();
      // The movie's box follows the canvas as the window changes.
      window.addEventListener('resize', moviePlace);
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
