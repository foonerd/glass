// Glass: the player's display, as a Volumio plugin.
//
// The audio path (ALSA chain, MPD side output, Spotify, AirPlay and DSP
// handling), the artist fanart cascade and the settings backups are carried
// over from PeppyMeter Screensaver by 2aCD (original) and Just a Nerd, MIT.

'use strict';

var libQ = require('kew');
var fs = require('fs-extra');
var exec = require('child_process').exec;
var execSync = require('child_process').execSync;
var sizeOf = require('image-size');
var crypto = require('crypto');
const lineReader = require('line-reader');
const net = require('net');
const io = require('socket.io-client');
const socket = io.connect('http://localhost:3000');
const path = require('path');
const ini = require('ini');
const pluginVersion = require('./package.json').version;

const id = 'glass: ';
const PluginPath = '/data/plugins/user_interface/glass';
const DATA_DIR = '/data/INTERNAL/glass';
// The run contract with the glass binary: it creates the run flag while a
// window is open and leaves when the flag goes; a real touch writes the
// dismiss marker; the persist file carries the countdown after a pause.
const runFlag = '/tmp/glass_running';
const persistFile = '/tmp/glass_persist';
const dismissFile = '/tmp/glass_dismiss';
const LaunchScript = PluginPath + '/run_glass.sh';
const ConfigDir = PluginPath + '/config';
const MeterConfigFile = ConfigDir + '/meter.txt';
const SpectrumConfigFile = ConfigDir + '/spectrum.txt';
const meterFolderStr = 'meter.folder';
const SpectrumFolderStr = 'spectrum.folder';

// The plugin Glass replaces. Only one of the two may be enabled at a time.
const LEGACY_PLUGIN = 'peppy_screensaver';
const LEGACY_CONFIG = '/data/configuration/user_interface/peppy_screensaver/config.json';
const LEGACY_METER_CONFIG = '/data/plugins/user_interface/peppy_screensaver/screensaver/peppymeter/config.txt';

// The channel to the display: a local socket the plugin serves, one JSON
// object per line. A display that connects gets a greeting, then the player's
// state and the infinity flag last seen, then every change as it comes; it
// sends commands for the player back.
const channelPath = '/tmp/glass_channel';
const CHANNEL_PROTOCOL = 1;
const CHANNEL_LINE_MAX = 65536;

function Channel(logger, onAttach, onCommand) {
    this.logger = logger;
    this.onAttach = onAttach;
    this.onCommand = onCommand;
    this.path = null;
    this.server = null;
    this.clients = [];
    this.state = null;
    this.infinity = null;
}

Channel.prototype.listen = function (path) {
    var self = this;
    self.path = path;
    try { if (fs.existsSync(path)) fs.unlinkSync(path); } catch (e) {}
    self.server = net.createServer(function (conn) { self.attach(conn); });
    self.server.on('error', function (err) {
        self.logger.error(id + 'channel: ' + (err && err.message ? err.message : err));
    });
    self.server.listen(path, function () {
        self.logger.info(id + 'channel: serving ' + path);
    });
};

Channel.prototype.attach = function (conn) {
    var self = this;
    var pending = '';
    conn.setEncoding('utf8');
    self.clients.push(conn);
    self.logger.info(id + 'channel: a display connected');
    self.tell(conn, { kind: 'hello', protocol: CHANNEL_PROTOCOL, plugin: pluginVersion });
    if (self.state) { self.tell(conn, { kind: 'state', state: self.state }); }
    if (self.infinity !== null) { self.tell(conn, { kind: 'infinity', on: self.infinity }); }
    conn.on('data', function (chunk) {
        pending += chunk;
        if (pending.length > CHANNEL_LINE_MAX) {
            self.logger.warn(id + 'channel: a line too long was dropped');
            pending = '';
            return;
        }
        var lines = pending.split('\n');
        pending = lines.pop();
        lines.forEach(function (line) {
            line = line.trim();
            if (!line) { return; }
            var message = null;
            try { message = JSON.parse(line); } catch (e) {}
            if (message && message.kind === 'command') {
                self.onCommand(message);
            } else {
                self.logger.warn(id + 'channel: not a command: ' + line.slice(0, 80));
            }
        });
    });
    var detach = function () {
        var at = self.clients.indexOf(conn);
        if (at !== -1) {
            self.clients.splice(at, 1);
            self.logger.info(id + 'channel: the display left');
        }
    };
    conn.on('error', detach);
    conn.on('close', detach);
    self.onAttach();
};

Channel.prototype.tell = function (conn, message) {
    try { conn.write(JSON.stringify(message) + '\n'); } catch (e) {}
};

// Every connected display hears this.
Channel.prototype.push = function (message) {
    var self = this;
    self.clients.slice().forEach(function (conn) { self.tell(conn, message); });
};

Channel.prototype.close = function () {
    var self = this;
    self.clients.slice().forEach(function (conn) { try { conn.destroy(); } catch (e) {} });
    self.clients = [];
    if (self.server) {
        try { self.server.close(); } catch (e) {}
        self.server = null;
    }
    try { if (self.path && fs.existsSync(self.path)) fs.unlinkSync(self.path); } catch (e) {}
};

// A user dismiss re-arms the full screensaver timeout. A clean exit without
// the marker (a settings reload) restarts now. A failed exit restarts now.
// An unarmed interval does neither.
function meterExitAction(cleanExit, timeoutArmed, dismissMarkerPresent) {
    if (!timeoutArmed) return 'idle';
    if (cleanExit && dismissMarkerPresent) return 'rearm';
    return 'restart';
}

// A display that dies this soon after launch is a crash, not a reload; the
// armed interval retries at the screensaver cadence.
var METER_CRASH_BACKOFF_MS = 10000;
function meterRestartNow(cleanExit, ranMs) {
    return cleanExit || ranMs >= METER_CRASH_BACKOFF_MS;
}

// theme-tag-contract:start
function lastEditionTag(text) {
    var last = '';
    var match;
    var re = /\[([^\[\]]+)\]/g;
    var source = String(text || '');
    while ((match = re.exec(source))) {
        last = match[1].trim();
    }
    return last;
}

function folderNameFromUri(uri) {
    var path = String(uri || '').split('?')[0];
    try { path = decodeURIComponent(path); } catch (e) {}
    var parts = path.split('/').filter(function (part) { return part !== ''; });
    if (parts.length < 2) return '';
    return parts[parts.length - 2];
}

function parseThemeTagRules(text) {
    var rules = [];
    String(text || '').split(/\r?\n|,/).forEach(function (line) {
        var eq = line.indexOf('=');
        if (eq <= 0) return;
        var tag = line.slice(0, eq).trim().toLowerCase();
        var folder = line.slice(eq + 1).trim();
        if (!tag || !folder) return;
        if (folder.indexOf('/') !== -1 || folder.indexOf('..') !== -1) return;
        rules.push({ tag: tag, folder: folder });
    });
    return rules;
}

// null: no rules, leave the theme alone. '': rules exist but nothing matched (use home).
function themeFolderForEdition(album, uri, rulesText) {
    var rules = parseThemeTagRules(rulesText);
    if (!rules.length) return null;
    var map = {};
    rules.forEach(function (rule) { map[rule.tag] = rule.folder; });
    var albumTag = lastEditionTag(album).toLowerCase();
    if (albumTag && map[albumTag]) return map[albumTag];
    var folderTag = lastEditionTag(folderNameFromUri(uri)).toLowerCase();
    if (folderTag && map[folderTag]) return map[folderTag];
    return '';
}
// theme-tag-contract:end

// Settings backups live under DATA_DIR and survive uninstall and reinstall.
// The file names inside a backup are the ones PeppyMeter Screensaver used,
// so a backup made there restores here.
const BackupsPath = DATA_DIR + '/backups';
const BackupSchemaVersion = 1;
const BackupNameRegex = /^[A-Za-z0-9 _.\-]{1,64}$/;
const BackupMinFreeBytes = 10 * 1024 * 1024;
const BackupWarnCount = 20;
const BackupManifestName = 'manifest.json';
const PeppyConfBackupName = 'peppymeter_config.txt';
const SpectrumConfBackupName = 'spectrum_config.txt';
const ThemeGalleryDir = PluginPath + '/theme-gallery';
const THEME_GALLERY_CACHE_EXTS = ['.png', '.jpg', '.jpeg'];
// Artist fanart: an on-disk cache served through /albumart?sectionimage=...
const FanartCacheDir = PluginPath + '/fanart-cache';
const FanartSectionPrefix = 'user_interface/glass/fanart-cache/';
const FanartPersonalArtDir = '/data/albumart/personal/artist';
const FANART_TV_PROJECT_KEY = '9bb4ee75161ec1245cb377bf2716b90b';
const FANART_TTL_MS = 14 * 24 * 60 * 60 * 1000;
const FANART_IMAGE_EXTS = ['.png', '.jpg', '.jpeg', '.webp'];
const FANART_MAX_IMAGES = 30;

var minmax = new Array(16);
var meterConfig, base_folder_P;
var spectrum_config, base_folder_S;
var availMeters = '';
var uiNeedsUpdate;
var remoteConfigVersion = '';
// The glass binary places its window and fades in on every backend.
var use_SDL2 = true;

const PluginConfiguration = '/data/configuration/plugins.json';
const MPDtmpl = '/volumio/app/plugins/music_service/mpd/mpd.conf.tmpl';
const MPD = '/tmp/mpd.conf.tmpl';
const MPD_include = '/data/configuration/music_service/mpd/mpd_custom.conf';
const AIRtmpl = '/volumio/app/plugins/music_service/airplay_emulation/shairport-sync.conf.tmpl';
const AIR = '/tmp/shairport-sync.conf.tmpl';
const asound = '/Glass.postGlass.5.conf';

const spotify_config = '/data/plugins/music_service/spop/config.yml.tmpl';

// Logging gated by the configuration's debug.level: basic, verbose, trace.
function levelAllows(level) {
    if (!meterConfig || !meterConfig.current) return false;
    var cfgLevel = meterConfig.current['debug.level'] || 'off';
    var levels = { 'off': 0, 'basic': 1, 'verbose': 2, 'trace': 3 };
    return (levels[cfgLevel] || 0) >= (levels[level] || 0);
}

function alsaLog(logger, level, msg) {
    if (levelAllows(level)) {
        logger.info(id + 'ALSA: ' + msg);
    }
}

function galleryLog(logger, level, msg) {
    if (levelAllows(level)) {
        logger.info(id + 'THEME: ' + msg);
    }
}

module.exports = Glass;

function Glass(context) {
    var self = this;
    self.context = context;
    self.commandRouter = self.context.coreCommand;
    self.logger = self.context.logger;
    self.configManager = self.context.configManager;
}

Glass.prototype.onVolumioStart = function () {
    var self = this;
    var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
    self.config = new (require('v-conf'))();
    self.config.loadFile(configFile);
    var defaults = {
        themeTagRules: ['string', ''],
        legacyImported: ['boolean', false],
        displayOutput: ['string', '0']
    };
    Object.keys(defaults).forEach(function (key) {
        if (self.config.get(key) === undefined) {
            self.config.addConfigValue(key, defaults[key][0], defaults[key][1]);
        }
    });
    return libQ.resolve();
};

// The player's architecture as the image names it: arm, armv7, armv8 or x64.
Glass.prototype.volumioArch = function () {
    try {
        return execSync('cat /etc/os-release | grep ^VOLUMIO_ARCH | tr -d \'VOLUMIO_ARCH="\'').toString().trim();
    } catch (e) {
        return '';
    }
};

// Read the meter and spectrum configurations the glass binary reads.
Glass.prototype.loadConfigs = function () {
    if (fs.existsSync(MeterConfigFile)) {
        meterConfig = ini.parse(fs.readFileSync(MeterConfigFile, 'utf-8'));
        base_folder_P = (meterConfig.current['base.folder'] || '') + '/';
        if (base_folder_P === '/') { base_folder_P = DATA_DIR + '/templates/'; }
    }
    if (fs.existsSync(SpectrumConfigFile)) {
        spectrum_config = ini.parse(fs.readFileSync(SpectrumConfigFile, 'utf-8'));
        base_folder_S = (spectrum_config.current['base.folder'] || '') + '/';
        if (base_folder_S === '/') { base_folder_S = DATA_DIR + '/templates_spectrum/'; }
    }
};

// The environment the display is launched with: its home, the X display
// the settings name, and the marker a real touch writes.
Glass.prototype.launchEnv = function () {
    var self = this;
    var display = String(self.config.get('displayOutput') || '0').replace(/[^0-9]/g, '') || '0';
    return Object.assign({}, process.env, {
        DISPLAY: ':' + display,
        GLASS_HOME: PluginPath,
        GLASS_DISMISS_FILE: dismissFile,
        GLASS_CHANNEL: channelPath
    });
};

// A command from the display, run through the player's command router.
// `seek` takes seconds, `volume` a number from 0 to 100 or one of the
// player's words (`+`, `-`, `mute`, `unmute`, `toggle`), `random` a
// boolean, `repeat` one of `off`, `all`, `single`.
Glass.prototype.runCommand = function (message) {
    var self = this;
    var router = self.commandRouter;
    var name = String(message.name || '');
    var value = message.value;
    self.logger.info(id + 'channel: command ' + name + (value !== undefined ? ' ' + JSON.stringify(value) : ''));
    try {
        switch (name) {
            case 'play': router.volumioPlay(); break;
            case 'pause': router.volumioPause(); break;
            case 'toggle': router.volumioToggle(); break;
            case 'stop': router.volumioStop(); break;
            case 'next': router.volumioNext(); break;
            case 'previous': router.volumioPrevious(); break;
            case 'seek': router.volumioSeek(Number(value) || 0); break;
            case 'volume': router.volumiosetvolume(typeof value === 'number' ? Math.round(value) : String(value)); break;
            case 'random': router.volumioRandom(!!value); break;
            case 'repeat': router.volumioRepeat(value === 'all' || value === 'single', value === 'single'); break;
            default: self.logger.warn(id + 'channel: unknown command ' + name);
        }
    } catch (e) {
        self.logger.error(id + 'channel: command ' + name + ' failed: ' + (e && e.message ? e.message : e));
    }
};

Glass.prototype.onStart = function () {
    var self = this;
    var defer = libQ.defer();
    var lastStateIsPlaying = false;
    self.Timeout = null;
    self.persistTimer = null;
    self.transitionGraceTimer = null;
    self.meterChild = null;

    // Strings are loaded here again so a fresh install needs no restart.
    self.commandRouter.loadI18nStrings();

    // Glass replaces PeppyMeter Screensaver: the two cannot share the audio path.
    if (self.legacyEnabled()) {
        self.refuseForLegacy();
        return libQ.reject(new Error('PeppyMeter Screensaver is enabled'));
    }

    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    try { if (fs.existsSync(dismissFile)) fs.removeSync(dismissFile); } catch (e) {}
    try { if (fs.existsSync(persistFile)) fs.removeSync(persistFile); } catch (e) {}

    // The channel the display reads the player's state from. A display that
    // connects gets fresh values asked of the player.
    self.channel = new Channel(self.logger, function () {
        socket.emit('getState', '');
        socket.emit('getInfinityPlayback', '');
    }, function (message) {
        self.runCommand(message);
    });
    self.channel.listen(channelPath);

    self.loadConfigs();
    if (!meterConfig) {
        self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
        return libQ.reject(new Error('meter configuration missing'));
    }
    try { self.importLegacySettings(); } catch (e) {
        self.logger.warn(id + 'settings import: ' + (e && e.message ? e.message : e));
    }

    // The audio path: the tap heads the ALSA contribution and meters every
    // source. What earlier releases added beside it is taken back once.
    self.retireSideOutputs(false)
        .then(self.writeAsoundConfigModular.bind(self))
        .then(self.updateALSAConfigFile.bind(self))
        .fail(function (e) {
            self.logger.error(id + 'audio path: ' + (e && e.message ? e.message : e));
        });

    // The player state drives the display: it opens after the timeout while
    // music plays, stays through a pause for the persist time, and leaves
    // when the run flag goes.
    socket.emit('getState', '');
    socket.emit('getInfinityPlayback', '');
    var lastService = '';
    var lastUri = '';

    socket.on('pushInfinityPlayback', function (data) {
        var on = !!(data && data.enabled);
        if (self.channel) {
            self.channel.infinity = on;
            self.channel.push({ kind: 'infinity', on: on });
        }
    });

    socket.on('pushState', function (state) {
        if (!state || typeof state !== 'object') {
            self.logger.warn(id + 'pushState: no state');
            return;
        }
        var status = state.status;
        if (status === undefined || status === null) {
            self.logger.warn(id + 'pushState: no status');
            return;
        }
        self.lastState = state;
        if (self.channel) {
            self.channel.state = state;
            self.channel.push({ kind: 'state', state: state });
        }
        self.logger.info(id + 'pushState: status=' + status + ' service=' + state.service + ' volatile=' + state.volatile);

        var persistDuration = parseInt(self.config.get('persist_duration'), 10) || 0;
        var persistDisplay = self.config.get('persist_display') || 'freeze';

        // volatile marks a transition between states. The empty state at the
        // end of the queue has status stop, no title and no volatile field.
        var isVolatile = state.volatile === true;
        var isGetEmptyState = (status === 'stop' && (state.uri || '') === '' && (state.title || '') === '');

        if (status === 'play') {
            if (self.persistTimer) {
                clearTimeout(self.persistTimer);
                self.persistTimer = null;
                self.logger.info(id + 'persist timer cancelled, playback resumed');
            }
            if (self.transitionGraceTimer) {
                clearTimeout(self.transitionGraceTimer);
                self.transitionGraceTimer = null;
            }
            try { if (fs.existsSync(persistFile)) fs.removeSync(persistFile); } catch (e) {}
            try { self.applyThemeTag(state); } catch (eTag) {
                self.logger.warn(id + 'theme tag: ' + (eTag && eTag.message ? eTag.message : eTag));
            }

            if (!self.Timeout) {
                lastStateIsPlaying = true;
                var ScreenTimeout = (parseInt(self.config.get('timeout'), 10)) * 1000;

                if (ScreenTimeout > 0) {
                    var startDisplayOnce = function () {
                        if (self.meterChild && self.meterChild.exitCode === null) {
                            return;
                        }
                        var child = exec(LaunchScript, { uid: 1000, gid: 1000, env: self.launchEnv() }, function (error, stdout, stderr) {
                            if (error !== null) {
                                self.logger.error(id + 'the display did not run: ' + error + (stderr ? ' ' + String(stderr).trim() : ''));
                            } else {
                                self.logger.info(id + 'the display ran and left');
                            }
                            var dismissMarkerPresent = false;
                            try { dismissMarkerPresent = fs.existsSync(dismissFile); } catch (e) {}
                            var action = meterExitAction(error === null, !!self.Timeout, dismissMarkerPresent);
                            try { if (dismissMarkerPresent) fs.removeSync(dismissFile); } catch (e) {}
                            if (self.meterChild === child) {
                                self.meterChild = null;
                                if (action === 'rearm') {
                                    clearInterval(self.Timeout);
                                    self.Timeout = setInterval(function () {
                                        startDisplayOnce();
                                    }, ScreenTimeout);
                                    self.logger.info(id + 'dismissed by touch, re-armed for ' + (ScreenTimeout / 1000) + ' s');
                                } else if (action === 'restart') {
                                    var ranMs = Date.now() - (child.startedAt || 0);
                                    if (meterRestartNow(error === null, ranMs)) {
                                        startDisplayOnce();
                                    } else {
                                        self.logger.warn(id + 'the display died ' + Math.round(ranMs / 1000) + ' s after launch; next attempt in ' + (ScreenTimeout / 1000) + ' s');
                                    }
                                }
                            }
                        });
                        child.startedAt = Date.now();
                        self.meterChild = child;
                    };
                    self.Timeout = setInterval(function () {
                        startDisplayOnce();
                    }, ScreenTimeout);
                }
            }
        } else if (lastStateIsPlaying) {
            // A pause and the end of the queue are genuine stops. A stop with
            // metadata may be a track change, so it gets a grace period in
            // which a following play cancels it.
            var genuineStop = function () {
                if (self.Timeout) {
                    clearInterval(self.Timeout);
                    self.Timeout = null;
                }
                if (persistDuration > 0 && fs.existsSync(runFlag)) {
                    if (self.persistTimer) {
                        clearTimeout(self.persistTimer);
                    }
                    self.logger.info(id + 'persist timer ' + persistDuration + ' s');
                    try {
                        fs.writeFileSync(persistFile, persistDuration + ':' + Date.now() + ':' + persistDisplay);
                    } catch (e) {}
                    self.persistTimer = setTimeout(function () {
                        self.persistTimer = null;
                        try { if (fs.existsSync(persistFile)) fs.removeSync(persistFile); } catch (e) {}
                        if (fs.existsSync(runFlag)) {
                            fs.removeSync(runFlag);
                            self.logger.info(id + 'persist timer expired, the display leaves');
                        }
                        lastStateIsPlaying = false;
                    }, persistDuration * 1000);
                } else {
                    if (fs.existsSync(runFlag)) {
                        fs.removeSync(runFlag);
                    }
                    lastStateIsPlaying = false;
                }
            };

            if (status === 'pause' || isGetEmptyState) {
                self.logger.info(id + (status === 'pause' ? 'paused' : 'end of queue'));
                if (self.transitionGraceTimer) {
                    clearTimeout(self.transitionGraceTimer);
                    self.transitionGraceTimer = null;
                }
                genuineStop();
            } else if (status === 'stop') {
                if (self.transitionGraceTimer) {
                    clearTimeout(self.transitionGraceTimer);
                }
                var TRANSITION_GRACE_MS = 5000;
                self.logger.info(id + 'stop with metadata, grace ' + TRANSITION_GRACE_MS + ' ms');
                self.transitionGraceTimer = setTimeout(function () {
                    self.transitionGraceTimer = null;
                    self.logger.info(id + 'grace over, a genuine stop');
                    genuineStop();
                }, TRANSITION_GRACE_MS);
            }

            // A volatile pause is a genuine persist and keeps the file; a
            // volatile stop or handoff clears it.
            if (isVolatile && status !== 'pause' && !isGetEmptyState) {
                try {
                    if (fs.existsSync(persistFile)) {
                        fs.removeSync(persistFile);
                        self.logger.info(id + 'volatile stop, persist file cleared');
                    }
                } catch (e) {}
            }
        }

        lastService = state.service || '';
        lastUri = state.uri || '';
    });

    // The artist fanart cascade the display asks for: personal art, the
    // album folder, fanart.tv, then Volumio's own source.
    self.commandRouter.addPluginRestEndpoint({
        endpoint: 'glass_artistfanart',
        type: 'user_interface',
        name: 'glass',
        method: 'getArtistFanart'
    });

    self.updateConfigVersion();
    // Volumio reads a plugin's ALSA contribution from the list it built at
    // start; a plugin installed since then joins the chain after a restart.
    setTimeout(function () { self.checkAlsaChain(); }, 12000);
    defer.resolve();
    return defer.promise;
};

// Whether the rebuilt ALSA chain carries the Glass section; if not, ask for
// a restart, once.
Glass.prototype.checkAlsaChain = function () {
    var self = this;
    var present = false;
    try { present = fs.readFileSync('/etc/asound.conf', 'utf8').indexOf('Glass section') !== -1; } catch (e) {}
    if (present || self.restartAsked) {
        return;
    }
    self.restartAsked = true;
    self.logger.warn(id + 'the ALSA chain does not carry Glass yet; a restart is needed');
    var name = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
    self.commandRouter.pushToastMessage('warning', name, self.commandRouter.getI18nString('GLASS.RESTART_MSG'));
    self.commandRouter.broadcastMessage('openModal', {
        title: self.commandRouter.getI18nString('GLASS.RESTART_TITLE'),
        message: self.commandRouter.getI18nString('GLASS.RESTART_MSG'),
        size: 'lg',
        buttons: [
            { name: self.commandRouter.getI18nString('COMMON.RESTART'), class: 'btn btn-info', emit: 'reboot', payload: '' },
            { name: self.commandRouter.getI18nString('COMMON.CONTINUE'), class: 'btn btn-default', emit: 'closeModals', payload: '' }
        ]
    });
};

Glass.prototype.onStop = function () {
    var self = this;

    self.commandRouter.stateMachine.stop().then(function () {
        if (self.Timeout) {
            clearInterval(self.Timeout);
            self.Timeout = null;
        }
        if (self.persistTimer) {
            clearTimeout(self.persistTimer);
            self.persistTimer = null;
        }
        if (self.transitionGraceTimer) {
            clearTimeout(self.transitionGraceTimer);
            self.transitionGraceTimer = null;
        }

        // The display leaves when the run flag goes.
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        try { if (fs.existsSync(dismissFile)) fs.removeSync(dismissFile); } catch (e) {}
        try { if (fs.existsSync(persistFile)) fs.removeSync(persistFile); } catch (e) {}

        self.commandRouter.removePluginRestEndpoint({ endpoint: 'glass_artistfanart' });
        socket.off('pushState');
        socket.off('pushInfinityPlayback');
        if (self.channel) {
            self.channel.close();
            self.channel = null;
        }
    });

    return libQ.resolve();
};

Glass.prototype.onRestart = function () {
};

Glass.prototype.onInstall = function () {
};

Glass.prototype.onUninstall = function () {
    var self = this;
    // Whatever an earlier release put beside the tap goes with it.
    self.retireSideOutputs(true);
};

// Whether PeppyMeter Screensaver is enabled, in which case Glass stays out
// of the audio path and does not start.
Glass.prototype.legacyEnabled = function () {
    var self = this;
    try {
        return self.commandRouter.pluginManager.isEnabled('user_interface', LEGACY_PLUGIN) === true;
    } catch (e) {
        return false;
    }
};

// Say why Glass did not start, with a way to fix it in one press. Glass
// disables itself, so the two are never both enabled at the next start and
// the ALSA chain is built without it.
Glass.prototype.refuseForLegacy = function () {
    var self = this;
    var name = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
    try {
        self.commandRouter.pluginManager.disablePlugin('user_interface', 'glass');
    } catch (e) {
        self.logger.warn(id + 'could not disable itself: ' + (e && e.message ? e.message : e));
    }
    self.commandRouter.pushToastMessage('error', name, self.commandRouter.getI18nString('GLASS.LEGACY_ENABLED_MSG'));
    self.commandRouter.broadcastMessage('openModal', {
        title: self.commandRouter.getI18nString('GLASS.LEGACY_ENABLED_TITLE'),
        message: self.commandRouter.getI18nString('GLASS.LEGACY_ENABLED_MSG'),
        size: 'lg',
        buttons: [
            {
                name: self.commandRouter.getI18nString('GLASS.LEGACY_DISABLE_BTN'),
                class: 'btn btn-warning',
                emit: 'callMethod',
                payload: { endpoint: 'user_interface/glass', method: 'disableLegacyAndStart', data: {} }
            },
            {
                name: self.commandRouter.getI18nString('COMMON.CANCEL'),
                class: 'btn btn-default',
                emit: 'closeModals',
                payload: ''
            }
        ]
    });
};

// Disable and stop PeppyMeter Screensaver, which also rebuilds the ALSA
// chain without it, then start Glass.
Glass.prototype.disableLegacyAndStart = function () {
    var self = this;
    var name = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
    self.commandRouter.closeModals();
    if (!self.legacyEnabled()) {
        return self.commandRouter.enableAndStartPlugin('user_interface', 'glass');
    }
    return libQ.resolve()
        .then(function () { return self.commandRouter.disableAndStopPlugin('user_interface', LEGACY_PLUGIN); })
        .then(function () {
            self.commandRouter.pushToastMessage('success', name, self.commandRouter.getI18nString('GLASS.LEGACY_DISABLED'));
            return self.commandRouter.enableAndStartPlugin('user_interface', 'glass');
        })
        .then(function () {
            uiNeedsUpdate = true;
            self.updateUIConfig();
        })
        .fail(function (e) {
            self.logger.error(id + 'disabling ' + LEGACY_PLUGIN + ': ' + (e && e.message ? e.message : e));
            self.commandRouter.pushToastMessage('error', name, self.commandRouter.getI18nString('GLASS.LEGACY_DISABLE_FAILED'));
        });
};

// Take the settings over from PeppyMeter Screensaver once: its plugin
// configuration when it is installed, and the meter configuration keys the
// display reads. Without an installed plugin, the newest named backup the
// installer adopted is restored instead.
Glass.prototype.importLegacySettings = function () {
    var self = this;
    if (self.config.get('legacyImported') === true) {
        return;
    }
    var imported = false;
    if (fs.existsSync(LEGACY_CONFIG)) {
        var legacy = new (require('v-conf'))();
        legacy.loadFile(LEGACY_CONFIG);
        ['timeout', 'persist_duration', 'persist_display', 'randomSelection', 'themeTagRules', 'displayOutput', 'doNotDeleteThemes', 'fanartEnabled', 'fanartKeyMode', 'fanart_personal_key', 'fanartInterval', 'fanartTransition', 'fanartTransitionMs', 'fanartOrder', 'fanartUnlimitedImages'].forEach(function (key) {
            var value = legacy.get(key);
            if (value !== undefined) {
                self.config.set(key, value);
            }
        });
        imported = true;
    }
    if (fs.existsSync(LEGACY_METER_CONFIG) && meterConfig) {
        var theirs = ini.parse(fs.readFileSync(LEGACY_METER_CONFIG, 'utf-8'));
        var keys = ['meter', 'meter.folder', 'random.meter.interval', 'random.change.title', 'frame.rate', 'position.type', 'position.x', 'position.y', 'start.animation', 'playinfo.type.mode', 'rotation.quality', 'reel.direction', 'rotation.fps', 'rotation.speed', 'spool.left.speed', 'spool.right.speed', 'spool.adaptive', 'scrolling.mode', 'scrolling.speed.artist', 'scrolling.speed.title', 'scrolling.speed.album', 'transition.type', 'transition.duration', 'transition.color', 'transition.opacity', 'queue.mode', 'exit.on.touch', 'stop.display.on.touch'];
        keys.forEach(function (key) {
            if (theirs.current && theirs.current[key] !== undefined) {
                meterConfig.current[key] = theirs.current[key];
            }
        });
        if (theirs['data.source'] && meterConfig['data.source']) {
            ['smooth.buffer.size', 'volume.gain.db', 'volume.min', 'volume.max', 'volume.constant', 'volume.max.in.pipe'].forEach(function (key) {
                if (theirs['data.source'][key] !== undefined) {
                    meterConfig['data.source'][key] = theirs['data.source'][key];
                }
            });
        }
        if (theirs['sdl.env'] && meterConfig['sdl.env'] && theirs['sdl.env']['mouse.enabled'] !== undefined) {
            meterConfig['sdl.env']['mouse.enabled'] = theirs['sdl.env']['mouse.enabled'];
        }
        // A theme the installer copied over keeps its place; one it did not have falls back to the bundled one.
        var folder = meterConfig.current[meterFolderStr];
        if (folder && !fs.existsSync(base_folder_P + folder)) {
            meterConfig.current[meterFolderStr] = '800x480';
            meterConfig.current.meter = 'random';
        }
        if (spectrum_config) {
            spectrum_config.current[SpectrumFolderStr] = meterConfig.current[meterFolderStr];
            fs.writeFileSync(SpectrumConfigFile, ini.stringify(spectrum_config, { whitespace: true }));
        }
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        self.config.set('activeFolder', meterConfig.current[meterFolderStr]);
        imported = true;
    } else if (!imported) {
        var backups = self.listSettingsBackups();
        if (backups.length > 0) {
            self.restoreSettingsBackup({ selectedBackup: { value: backups[0].name }, quiet: true });
            imported = true;
        }
    }
    self.config.set('legacyImported', true);
    if (imported) {
        self.logger.info(id + 'settings taken over from ' + LEGACY_PLUGIN);
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.IMPORTED_SETTINGS'));
    }
};

Glass.prototype.getUIConfig = function () {
    var defer = libQ.defer();
    var self = this;
    var lang_code = self.commandRouter.sharedVars.get('language_code');

    self.commandRouter.i18nJson(__dirname + '/i18n/strings_' + lang_code + '.json',
        __dirname + '/i18n/strings_en.json',
        __dirname + '/UIConfig.json')
        .then(function (uiconf) {
            // A control by its id, wherever it sits.
            var C = function (controlId) {
                for (var si = 0; si < uiconf.sections.length; si++) {
                    var content = uiconf.sections[si] && uiconf.sections[si].content;
                    if (!content) { continue; }
                    for (var ci = 0; ci < content.length; ci++) {
                        if (content[ci] && content[ci].id === controlId) { return content[ci]; }
                    }
                }
                self.logger.error(id + 'getUIConfig: control id not found: ' + controlId);
                return { value: {}, options: [], attributes: [{}, {}, {}, {}] };
            };
            // The configManager path of a control's options, for pushUIConfigParam.
            var P = function (controlId) {
                for (var si = 0; si < uiconf.sections.length; si++) {
                    var content = uiconf.sections[si] && uiconf.sections[si].content;
                    if (!content) { continue; }
                    for (var ci = 0; ci < content.length; ci++) {
                        if (content[ci] && content[ci].id === controlId) {
                            return 'sections[' + si + '].content[' + ci + '].options';
                        }
                    }
                }
                self.logger.error(id + 'getUIConfig: option path not found: ' + controlId);
                return 'sections[0].content[0].options';
            };
            var pick = function (controlId, value) {
                var options = C(controlId).options || [];
                for (var i = 0; i < options.length; i++) {
                    if (options[i].value === value) {
                        C(controlId).value = options[i];
                        return;
                    }
                }
            };

            // The guard section shows only while PeppyMeter Screensaver is enabled.
            if (!self.legacyEnabled()) {
                uiconf.sections = uiconf.sections.filter(function (s) { return s.id !== 'legacy_conf'; });
            }

            self.loadConfigs();
            if (!meterConfig) {
                self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
                defer.resolve(uiconf);
                return;
            }
            var meters_file = base_folder_P + meterConfig.current[meterFolderStr] + '/meters.txt';
            var upperc = /\b([^-])/g;

            // Display and activation.
            C('timeout').value = self.config.get('timeout');
            minmax[0] = [C('timeout').attributes[2].min, C('timeout').attributes[3].max, C('timeout').attributes[0].placeholder];
            if (meterConfig.current['position.type'] == 'center') {
                C('positionType').value.value = 0;
                C('positionType').value.label = 'centered';
            } else {
                C('positionType').value.value = 1;
                C('positionType').value.label = 'manually';
            }
            C('position_x').value = parseInt(meterConfig.current['position.x'], 10) || 0;
            minmax[1] = [C('position_x').attributes[2].min, C('position_x').attributes[3].max, C('position_x').attributes[0].placeholder];
            C('position_y').value = parseInt(meterConfig.current['position.y'], 10) || 0;
            minmax[2] = [C('position_y').attributes[2].min, C('position_y').attributes[3].max, C('position_y').attributes[0].placeholder];
            var mouseSupport = String(meterConfig.sdl.env['mouse.enabled'] || 'True').toLowerCase() == 'true';
            C('mouseEnabled').value = mouseSupport;
            C('displayOutput').value.value = self.config.get('displayOutput');
            C('displayOutput').value.label = 'Display=' + self.config.get('displayOutput');
            var formatTypeMode = meterConfig.current['playinfo.type.mode'] || 'icon';
            if (formatTypeMode !== 'icon' && formatTypeMode !== 'text' && formatTypeMode !== 'both') {
                formatTypeMode = 'icon';
            }
            pick('formatTypeMode', formatTypeMode);

            // Themes and artwork.
            self.configManager.pushUIConfigParam(uiconf, P('themeToRemove'), {
                value: '',
                label: self.commandRouter.getI18nString('GLASS.THEME_REMOVE_NONE')
            });
            var files = [];
            try { files = fs.readdirSync(base_folder_P); } catch (e) {}
            files.forEach(function (file) {
                var stat;
                try { stat = fs.statSync(base_folder_P + file); } catch (e) { return; }
                if (!stat.isDirectory()) { return; }
                var str_empty = fs.existsSync(base_folder_P + file + '/meters.txt') ? '' : ' (empty)';
                var folderLabel = file + str_empty;
                if (file.includes('_')) {
                    var partFile = file.split('_');
                    folderLabel = (partFile[1] || '').replace(upperc, function (c) { return c.toUpperCase(); }) + '-' + (partFile[2] || '') + ' ' + partFile[0] + str_empty;
                }
                self.configManager.pushUIConfigParam(uiconf, P('activeFolder'), { value: file, label: folderLabel });
                self.configManager.pushUIConfigParam(uiconf, P('themeToRemove'), { value: file, label: folderLabel });
            });
            C('themeToRemove').value.value = '';
            C('themeToRemove').value.label = self.commandRouter.getI18nString('GLASS.THEME_REMOVE_NONE');
            var meterFolder = meterConfig.current[meterFolderStr] || '';
            C('activeFolder').value.value = meterFolder;
            if (meterFolder.includes('_')) {
                var part = meterFolder.split('_');
                C('activeFolder').value.label = (part[1] || '').replace(upperc, function (c) { return c.toUpperCase(); }) + '-' + (part[2] || '') + ' ' + part[0];
            } else {
                C('activeFolder').value.label = meterFolder;
            }
            C('themeTagRules').value = self.config.get('themeTagRules') || '';
            C('doNotDeleteThemes').value = self.config.get('doNotDeleteThemes') === true;
            C('fanartEnabled').value = self.config.get('fanartEnabled') === true;
            var fanartKeyMode = self.config.get('fanartKeyMode') || 'personal';
            C('fanartKeyMode').value.value = fanartKeyMode;
            C('fanartKeyMode').value.label = self.commandRouter.getI18nString(fanartKeyMode === 'project' ? 'GLASS.FANART_KEY_MODE_PROJECT' : 'GLASS.FANART_KEY_MODE_PERSONAL');
            C('fanart_personal_key').value = self.config.get('fanart_personal_key') || '';
            C('fanartInterval').value = parseInt(self.config.get('fanartInterval'), 10) || 0;
            var fanartOrder = self.config.get('fanartOrder') || 'sequential';
            if (['sequential', 'random'].indexOf(fanartOrder) === -1) { fanartOrder = 'sequential'; }
            C('fanartOrder').value.value = fanartOrder;
            C('fanartOrder').value.label = self.commandRouter.getI18nString(fanartOrder === 'random' ? 'GLASS.FANART_ORDER_RANDOM' : 'GLASS.FANART_ORDER_SEQUENTIAL');
            var fanartTransition = self.config.get('fanartTransition') || 'none';
            var fanartTransitionLabels = { none: 'GLASS.FANART_TRANSITION_NONE', fade: 'GLASS.FANART_TRANSITION_FADE', merge: 'GLASS.FANART_TRANSITION_MERGE' };
            C('fanartTransition').value.value = fanartTransition;
            C('fanartTransition').value.label = self.commandRouter.getI18nString(fanartTransitionLabels[fanartTransition] || fanartTransitionLabels.none);
            C('fanartTransitionMs').value = parseInt(self.config.get('fanartTransitionMs'), 10) || 600;
            C('fanartUnlimitedImages').value = self.config.get('fanartUnlimitedImages') === true;

            // Meter.
            C('smoothBuffer').value = parseInt(meterConfig.data.source['smooth.buffer.size'], 10) || 0;
            minmax[3] = [C('smoothBuffer').attributes[2].min, C('smoothBuffer').attributes[3].max, C('smoothBuffer').attributes[0].placeholder];
            var meterGainVal = parseInt(meterConfig.data.source['volume.gain.db'], 10);
            C('meterGain').value = Number.isFinite(meterGainVal) ? meterGainVal : 0;
            minmax[15] = [C('meterGain').attributes[2].min, C('meterGain').attributes[3].max, C('meterGain').attributes[0].placeholder];
            availMeters = '';
            if (fs.existsSync(meters_file)) {
                var metersconfig = ini.parse(fs.readFileSync(meters_file, 'utf-8'));
                if (String(meterConfig.current.meter).includes(',')) {
                    C('meter').value.value = 'list';
                } else {
                    C('meter').value.value = meterConfig.current.meter;
                }
                C('meter').value.label = String(C('meter').value.value).replace(upperc, function (c) { return c.toUpperCase(); });
                for (var section in metersconfig) {
                    availMeters += section + ', ';
                    self.configManager.pushUIConfigParam(uiconf, P('meter'), {
                        value: section,
                        label: section.replace(upperc, function (c) { return c.toUpperCase(); })
                    });
                }
                availMeters = availMeters.substring(0, availMeters.length - 2);
                if (self.config.get('randomSelection') == '') {
                    C('randomSelection').value = availMeters;
                } else {
                    C('randomSelection').value = self.config.get('randomSelection');
                }
                C('randomSelection').doc = self.commandRouter.getI18nString('GLASS.RANDOMSELECTION_DOC') + '<b>' + availMeters + '</b>';
                if (C('meter').value.value == 'random' || C('meter').value.value == 'list') {
                    C('randomMode').hidden = false;
                }
                var random_change_title = String(meterConfig.current['random.change.title'] || 'False').toLowerCase() == 'true';
                if (random_change_title) {
                    C('randomMode').value.value = 'titlechange';
                    C('randomMode').value.label = 'On Title Change';
                } else {
                    C('randomMode').value.value = 'interval';
                    C('randomMode').value.label = 'Interval';
                }
                C('randomInterval').value = parseInt(meterConfig.current['random.meter.interval'], 10) || 60;
                minmax[5] = [C('randomInterval').attributes[2].min, C('randomInterval').attributes[3].max, C('randomInterval').attributes[0].placeholder];
            }

            // Frame rate.
            C('frameRate').value = parseInt(meterConfig.current['frame.rate'], 10) || 30;
            minmax[6] = [C('frameRate').attributes[2].min, C('frameRate').attributes[3].max, C('frameRate').attributes[0].placeholder];

            // Text scrolling.
            pick('scrollingMode', meterConfig.current['scrolling.mode'] || 'skin');
            C('scrollingSpeedArtist').value = parseInt(meterConfig.current['scrolling.speed.artist'], 10) || 40;
            C('scrollingSpeedTitle').value = parseInt(meterConfig.current['scrolling.speed.title'], 10) || 40;
            C('scrollingSpeedAlbum').value = parseInt(meterConfig.current['scrolling.speed.album'], 10) || 40;

            // Animation.
            C('animation').value = String(meterConfig.current['start.animation'] || 'False').toLowerCase() == 'true';
            pick('transitionType', meterConfig.current['transition.type'] || 'fade');
            var transitionDuration = parseFloat(meterConfig.current['transition.duration']);
            if (isNaN(transitionDuration)) { transitionDuration = 0.5; }
            C('transitionDuration').value = transitionDuration;
            minmax[9] = [C('transitionDuration').attributes[2].min, C('transitionDuration').attributes[3].max, C('transitionDuration').attributes[0].placeholder];
            pick('transitionColor', meterConfig.current['transition.color'] || 'black');
            var transitionOpacity = parseInt(meterConfig.current['transition.opacity'], 10);
            if (isNaN(transitionOpacity)) transitionOpacity = 100;
            C('transitionOpacity').value = transitionOpacity;
            minmax[10] = [C('transitionOpacity').attributes[2].min, C('transitionOpacity').attributes[3].max, C('transitionOpacity').attributes[0].placeholder];

            // Rotation and reels.
            pick('rotationQuality', meterConfig.current['rotation.quality'] || 'medium');
            C('rotationFPS').value = parseInt(meterConfig.current['rotation.fps'], 10) || 8;
            minmax[11] = [C('rotationFPS').attributes[2].min, C('rotationFPS').attributes[3].max, C('rotationFPS').attributes[0].placeholder];
            var rotationSpeed = parseFloat(meterConfig.current['rotation.speed']);
            if (isNaN(rotationSpeed)) { rotationSpeed = 1.0; }
            C('rotationSpeed').value = rotationSpeed;
            minmax[12] = [C('rotationSpeed').attributes[2].min, C('rotationSpeed').attributes[3].max, C('rotationSpeed').attributes[0].placeholder];
            var spoolLeftSpeed = parseFloat(meterConfig.current['spool.left.speed']);
            if (isNaN(spoolLeftSpeed)) { spoolLeftSpeed = 1.0; }
            C('spoolLeftSpeed').value = spoolLeftSpeed;
            minmax[13] = [C('spoolLeftSpeed').attributes[2].min, C('spoolLeftSpeed').attributes[3].max, C('spoolLeftSpeed').attributes[0].placeholder];
            var spoolRightSpeed = parseFloat(meterConfig.current['spool.right.speed']);
            if (isNaN(spoolRightSpeed)) { spoolRightSpeed = 1.0; }
            C('spoolRightSpeed').value = spoolRightSpeed;
            minmax[14] = [C('spoolRightSpeed').attributes[2].min, C('spoolRightSpeed').attributes[3].max, C('spoolRightSpeed').attributes[0].placeholder];
            C('spoolAdaptive').value = meterConfig.current['spool.adaptive'] === true || meterConfig.current['spool.adaptive'] === 'true';
            pick('reelDirection', meterConfig.current['reel.direction'] || 'ccw');

            // Playback.
            var persistVal = self.config.get('persist_duration');
            if (persistVal === undefined || persistVal === null || persistVal === '') { persistVal = '30'; }
            persistVal = String(persistVal);
            var persistLabels = { '0': 'GLASS.PERSIST_DISABLED', '5': 'GLASS.PERSIST_5', '15': 'GLASS.PERSIST_15', '30': 'GLASS.PERSIST_30', '60': 'GLASS.PERSIST_60', '120': 'GLASS.PERSIST_120', '300': 'GLASS.PERSIST_300' };
            C('persist_duration').value.value = persistVal;
            C('persist_duration').value.label = self.commandRouter.getI18nString(persistLabels[persistVal] || 'GLASS.PERSIST_30');
            var persistDisplayVal = self.config.get('persist_display') || 'freeze';
            C('persist_display').value.value = persistDisplayVal;
            C('persist_display').value.label = self.commandRouter.getI18nString(persistDisplayVal === 'countdown' ? 'GLASS.PERSIST_DISPLAY_COUNTDOWN' : 'GLASS.PERSIST_DISPLAY_FREEZE');
            var queueMode = meterConfig.current['queue.mode'] || 'track';
            pick('queueMode', queueMode);

            // Backups.
            var backupList = self.listSettingsBackups();
            if (backupList.length > 0) {
                var firstLabel = backupList[0].name + ' (' + backupList[0].createdLabel + ', v' + backupList[0].pluginVersion + ')';
                var firstValue = { value: backupList[0].name, label: firstLabel };
                C('selectedBackup').options = [];
                C('selectedBackupDelete').options = [];
                backupList.forEach(function (b) {
                    var entry = { value: b.name, label: b.name + ' (' + b.createdLabel + ', v' + b.pluginVersion + ')' };
                    self.configManager.pushUIConfigParam(uiconf, P('selectedBackup'), entry);
                    self.configManager.pushUIConfigParam(uiconf, P('selectedBackupDelete'), entry);
                });
                C('selectedBackup').value = firstValue;
                C('selectedBackupDelete').value = firstValue;
            }

            defer.resolve(uiconf);
        })
        .fail(function (e) {
            self.logger.error(id + 'getUIConfig: ' + (e && e.message ? e.message : e));
            defer.reject(new Error());
        });
    return defer.promise;
};

Glass.prototype.getConfigurationFiles = function() {
	return ['config.json'];
};

// Audio Source save handler (split out of the old global section). Selects which
// audio stream the meters react to (ALSA capture / DSP / Spotify / AirPlay). All of
// these live in config.json and may trigger an ALSA rebuild; a change reloads the
// meter so it captures from the new source.
//-------------------------------------------------------


// Display & Activation save handler (split out of the old global section). Covers the
// screensaver activation timeout (config.json), on-screen position + mouse pointer
// (config.txt, SDL2) and the display output (config.json). Any change reloads the
// running meter so it is re-rendered with the new geometry/output.
//-------------------------------------------------------

Glass.prototype.saveDisplayConf = function (confData) {
  const self = this;
  let noChanges = true;
  uiNeedsUpdate = false;

  // write timeout (config.json)
  if (Number.isNaN(parseInt(confData.timeout, 10)) || !isFinite(confData.timeout)) {
      uiNeedsUpdate = true;
      setTimeout(function () {
          self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.TIMEOUT') + self.commandRouter.getI18nString('GLASS.NAN'));
      }, 500);
  } else {
      confData.timeout = self.minmax('TIMEOUT', confData.timeout, minmax[0]);
      if (confData.timeout != self.config.get('timeout')){
          self.config.set('timeout', confData.timeout);
          noChanges = false;
      }
  }

  if (fs.existsSync(MeterConfigFile)){

    // write position type
    var pos_type = use_SDL2 ? confData.positionType.value == 0? 'center' : 'manual' : 'center';
    if (meterConfig.current['position.type'] !== pos_type) {
        meterConfig.current['position.type'] = pos_type;
        noChanges = false;
    }
    if (use_SDL2) {
        // write position x
        if (Number.isNaN(parseInt(confData.position_x, 10)) || !isFinite(confData.position_x)) {
            uiNeedsUpdate = true;
            setTimeout(function () {
                self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.POS_X') + self.commandRouter.getI18nString('GLASS.NAN'));
            }, 500);
        } else {
            confData.position_x = self.minmax('POS_X', confData.position_x, minmax[1]);
            if (meterConfig.current['position.x'] != confData.position_x) {
                meterConfig.current['position.x'] = confData.position_x;
                noChanges = false;
            }
        }
        // write position y
        if (Number.isNaN(parseInt(confData.position_y, 10)) || !isFinite(confData.position_y)) {
            uiNeedsUpdate = true;
            setTimeout(function () {
                self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.POS_Y') + self.commandRouter.getI18nString('GLASS.NAN'));
            }, 500);
        } else {
            confData.position_y = self.minmax('POS_Y', confData.position_y, minmax[2]);
            if (meterConfig.current['position.y'] != confData.position_y) {
                meterConfig.current['position.y'] = confData.position_y;
                noChanges = false;
            }
        }
    }

    // write mouse support
    var mouseSupport = confData.mouseEnabled? 'True' : 'False';
    if (meterConfig.sdl.env['mouse.enabled'] != mouseSupport) {
        meterConfig.sdl.env['mouse.enabled'] = mouseSupport;
        noChanges = false;
    }

    // write display port (config.json + live switch)
    if (self.config.get('displayOutput') != confData.displayOutput.value) {
        self.config.set('displayOutput', confData.displayOutput.value);
        var DispOut = parseInt(confData.displayOutput.value,10);
        self.switch_DisplayPort(DispOut);
        noChanges = false;
    }

    // write format / type display mode (like scrolling.mode: config.txt [current] only)
    var formatTypeMode = (confData.formatTypeMode && confData.formatTypeMode.value) || 'icon';
    if (formatTypeMode !== 'icon' && formatTypeMode !== 'text' && formatTypeMode !== 'both') {
        formatTypeMode = 'icon';
    }
    if (meterConfig.current['playinfo.type.mode'] != formatTypeMode) {
        meterConfig.current['playinfo.type.mode'] = formatTypeMode;
        noChanges = false;
    }

    if (!noChanges) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
        // Restart meter to apply new settings
        if (fs.existsSync(runFlag)){fs.removeSync(runFlag);}
    }
  } else {
      self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
  }

  if (uiNeedsUpdate) {self.updateUIConfig();}

  setTimeout(function () {
    if (noChanges) {
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
    } else {
        self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    }
  }, 500);
}; // end saveDisplayConf ----------------------------

// Template File Sharing (SMB) save handler.
// Only toggles config.json + filesystem permissions used for sharing templates over
// the network share; it has no effect on the running meter, so no reload is needed.
// ---------------------------------------------------------------

Glass.prototype.savePlaybackConf = function(data) {
    var self = this;
    var defer = libQ.defer();
    
    // Handle 0 (Disabled) as valid value - check for undefined/null, not truthiness
    var persistDuration = (data['persist_duration'] && data['persist_duration'].value !== undefined) 
        ? String(data['persist_duration'].value)
        : '30';
    
    var persistDisplay = data['persist_display'] && data['persist_display'].value 
        ? data['persist_display'].value 
        : 'freeze';
    
    // Queue mode - save to both config.json and MeterConfigFile
    var queueMode = data['queueMode'] && data['queueMode'].value 
        ? data['queueMode'].value 
        : 'track';
    
    // Validate queue mode
    if (queueMode !== 'track' && queueMode !== 'queue') {
        queueMode = 'track';  // Default fallback
    }
    
    self.config.set('persist_duration', persistDuration);
    self.config.set('persist_display', persistDisplay);
    
    // Track if queue mode changed (needs restart to apply)
    var queueModeChanged = false;
    
    // Save queue mode to MeterConfigFile (for Python handlers)
    if (fs.existsSync(MeterConfigFile)) {
        if (meterConfig.current['queue.mode'] != queueMode) {
            meterConfig.current['queue.mode'] = queueMode;
            fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
            queueModeChanged = true;
        }
    }
    
    // Restart meter to apply new queue mode setting
    if (queueModeChanged && fs.existsSync(runFlag)) {
        fs.removeSync(runFlag);
    }
    
    self.commandRouter.pushToastMessage('success', 
        self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), 
        self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    
    defer.resolve();
    return defer.promise;
};

// called when 'save' button pressed on VU-Meter settings
// ------------------------------------------------------

Glass.prototype.saveVUMeterConf = function (confData) {
  const self = this;
  let noChanges = true;
  uiNeedsUpdate = false;
  
  if (fs.existsSync(MeterConfigFile)){
    //var config = ini.parse(fs.readFileSync(MeterConfigFile, 'utf-8'));
    
    // write selected meter
    if ((confData.meter.value !== 'list' && meterConfig.current.meter !== confData.meter.value) || (confData.meter.value == 'list' && meterConfig.current.meter !== confData.randomSelection)) {
        if (confData.meter.value === 'list') {
            if (confData.randomSelection !== ''){
				if (self.checkListMode(confData.randomSelection)) {
                    meterConfig.current.meter = (confData.randomSelection);
                    self.config.set('randomSelection', (confData.randomSelection));
                }
            } else {
                meterConfig.current.meter = availMeters;
                self.config.set('randomSelection', availMeters);
            }
        } else {
            meterConfig.current.meter = confData.meter.value;
        }
        uiNeedsUpdate = true;
        noChanges = false;
    }

    // write random mode
    var random_change_title = (meterConfig.current['random.change.title']).toLowerCase() == 'true' ? true : false;
    if ((confData.randomMode.value == 'titlechange' && !random_change_title) || (confData.randomMode.value == 'interval' && random_change_title)){
        if (confData.randomMode.value == 'titlechange') {
            meterConfig.current['random.change.title'] = 'True';
        } else {
            meterConfig.current['random.change.title'] = 'False';
        }
        uiNeedsUpdate = true;
        noChanges = false;    
    }
    
    // write random interval
    if (Number.isNaN(parseInt(confData.randomInterval, 10)) || !isFinite(confData.randomInterval)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.RANDOMINTERVAL') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.randomInterval = self.minmax('RANDOMINTERVAL', confData.randomInterval, minmax[5]);
        if (meterConfig.current['random.meter.interval'] != confData.randomInterval) {
            meterConfig.current['random.meter.interval'] = confData.randomInterval;
            noChanges = false;
        }
    }

    // smooth buffer (moved here from the global section: meter feel)
    if (Number.isNaN(parseInt(confData.smoothBuffer, 10)) || !isFinite(confData.smoothBuffer)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.SMOOTH_BUFFER') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.smoothBuffer = self.minmax('SMOOTH_BUFFER', confData.smoothBuffer, minmax[3]);
        if (meterConfig.data.source['smooth.buffer.size'] != confData.smoothBuffer) {
            meterConfig.data.source['smooth.buffer.size'] = confData.smoothBuffer;
            noChanges = false;
        }
    }

    // meter sensitivity (gain in dB, consumed by the data source)
    if (Number.isNaN(parseInt(confData.meterGain, 10)) || !isFinite(confData.meterGain)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.METER_SENSITIVITY') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.meterGain = self.minmax('METER_SENSITIVITY', confData.meterGain, minmax[15]);
        if (meterConfig.data.source['volume.gain.db'] != confData.meterGain) {
            meterConfig.data.source['volume.gain.db'] = confData.meterGain;
            noChanges = false;
        }
    }
    
    if (!noChanges) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
        try { self.updateConfigVersion(); } catch (e) {}
        // Restart meter to apply new settings
        if (fs.existsSync(runFlag)){fs.removeSync(runFlag);}
    }
  } else {
      self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
  }
  
  if (uiNeedsUpdate) {self.updateUIConfig();}
  setTimeout(function () {
    if (noChanges) {
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
    } else {
        self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    }
  }, 500);
}; // end saveVUMeterConf -------------------------------------

// called when 'save' button pressed on Performance settings
// ----------------------------------------------------------

// The frame rate the display draws at.
Glass.prototype.savePerformanceConf = function (confData) {
    var self = this;
    var noChanges = true;
    uiNeedsUpdate = false;
    if (fs.existsSync(MeterConfigFile)) {
        if (Number.isNaN(parseInt(confData.frameRate, 10)) || !isFinite(confData.frameRate)) {
            uiNeedsUpdate = true;
            setTimeout(function () {
                self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.FRAME_RATE') + self.commandRouter.getI18nString('GLASS.NAN'));
            }, 500);
        } else {
            confData.frameRate = self.minmax('FRAME_RATE', confData.frameRate, minmax[6]);
            if (meterConfig.current['frame.rate'] != confData.frameRate) {
                meterConfig.current['frame.rate'] = confData.frameRate;
                noChanges = false;
            }
        }
        if (!noChanges) {
            fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
            if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        }
    } else {
        self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
    }
    if (uiNeedsUpdate) { self.updateUIConfig(); }
    setTimeout(function () {
        if (noChanges) {
            self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
        } else {
            self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
        }
    }, 500);
};

// Themes and artwork: the active theme, the tag rules, whether themes
// survive an uninstall, and the artist fanart slideshow.
Glass.prototype.saveThemesArtwork = function (data) {
    var self = this;
    var defer = libQ.defer();
    var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
    var noChanges = true;
    try {
        var folder = (data && data.activeFolder && typeof data.activeFolder === 'object') ? data.activeFolder.value : (data && data.activeFolder);
        if (folder && meterConfig && folder !== meterConfig.current[meterFolderStr]) {
            var result = self.applyActiveThemeFolder(folder, { allowBuiltin: true });
            if (result && result.changed) {
                noChanges = false;
            } else if (result && result.error) {
                self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.THEME_GALLERY_INVALID'));
            }
        }
        var rules = (data && typeof data.themeTagRules === 'string') ? data.themeTagRules : '';
        if ((self.config.get('themeTagRules') || '') !== rules) {
            self.config.set('themeTagRules', rules);
            noChanges = false;
            try { self.applyThemeTag(self.lastState); } catch (e) {}
        }
        var preserve = !!(data && (data.doNotDeleteThemes === true || data.doNotDeleteThemes === 'true'));
        if ((self.config.get('doNotDeleteThemes') === true) !== preserve) {
            self.config.set('doNotDeleteThemes', preserve);
            noChanges = false;
        }
        self.syncPreserveFlag(preserve);

        var enabled = !!(data && (data.fanartEnabled === true || data.fanartEnabled === 'true'));
        var keyMode = (data && data.fanartKeyMode && typeof data.fanartKeyMode === 'object') ? data.fanartKeyMode.value : (data && data.fanartKeyMode);
        if (keyMode !== 'project') { keyMode = 'personal'; }
        var key = (data && typeof data.fanart_personal_key === 'string') ? data.fanart_personal_key.trim() : '';
        var interval = parseInt(data && data.fanartInterval, 10);
        if (isNaN(interval) || interval < 0) { interval = 0; }
        if (interval > 3600) { interval = 3600; }
        var transition = (data && data.fanartTransition && typeof data.fanartTransition === 'object') ? data.fanartTransition.value : (data && data.fanartTransition);
        if (['none', 'fade', 'merge'].indexOf(transition) === -1) { transition = 'none'; }
        var transitionMs = parseInt(data && data.fanartTransitionMs, 10);
        if (isNaN(transitionMs) || transitionMs < 50) { transitionMs = 600; }
        if (transitionMs > 3000) { transitionMs = 3000; }
        var order = (data && data.fanartOrder && typeof data.fanartOrder === 'object') ? data.fanartOrder.value : (data && data.fanartOrder);
        if (['sequential', 'random'].indexOf(order) === -1) { order = 'sequential'; }
        var unlimited = !!(data && (data.fanartUnlimitedImages === true || data.fanartUnlimitedImages === 'true'));
        var wanted = { fanartEnabled: enabled, fanartKeyMode: keyMode, fanart_personal_key: key, fanartInterval: interval, fanartTransition: transition, fanartTransitionMs: transitionMs, fanartOrder: order, fanartUnlimitedImages: unlimited };
        Object.keys(wanted).forEach(function (k) {
            if (self.config.get(k) !== wanted[k]) {
                self.config.set(k, wanted[k]);
                noChanges = false;
            }
        });
        if (!noChanges && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        if (noChanges) {
            self.commandRouter.pushToastMessage('info', pluginName, self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
        } else {
            self.commandRouter.pushToastMessage('success', pluginName, self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
        }
    } catch (e) {
        self.logger.error(id + 'saveThemesArtwork: ' + (e && e.message ? e.message : e));
        self.commandRouter.pushToastMessage('error', pluginName, String(e && e.message ? e.message : e));
    }
    defer.resolve();
    return defer.promise;
};

// The marker the uninstall script honours: with it, the themes stay.
Glass.prototype.syncPreserveFlag = function (preserve) {
    var self = this;
    try {
        fs.ensureDirSync(DATA_DIR);
        if (preserve) {
            fs.writeFileSync(DATA_DIR + '/.preserve', '', 'utf8');
        } else if (fs.existsSync(DATA_DIR + '/.preserve')) {
            fs.unlinkSync(DATA_DIR + '/.preserve');
        }
    } catch (e) {
        self.logger.warn(id + 'preserve flag: ' + (e && e.message ? e.message : e));
    }
};

// The X display comes from the settings at every launch; nothing to rewrite.
Glass.prototype.switch_DisplayPort = function (DispOut) {
    var self = this;
    self.logger.info(id + 'display :' + DispOut + ' from the next launch');
    return libQ.resolve();
};

Glass.prototype.saveScrollingConf = function (confData) {
  const self = this;
  let noChanges = true;
  uiNeedsUpdate = false;
  
  if (fs.existsSync(MeterConfigFile)){
    
    // write scrolling mode
    var scrollingMode = confData.scrollingMode.value || 'skin';
    if (meterConfig.current['scrolling.mode'] != scrollingMode) {
        meterConfig.current['scrolling.mode'] = scrollingMode;
        noChanges = false;
    }
    
    // write scrolling speed artist
    var scrollSpeedArtist = parseInt(confData.scrollingSpeedArtist, 10) || 40;
    if (meterConfig.current['scrolling.speed.artist'] != scrollSpeedArtist) {
        meterConfig.current['scrolling.speed.artist'] = scrollSpeedArtist;
        noChanges = false;
    }
    
    // write scrolling speed title
    var scrollSpeedTitle = parseInt(confData.scrollingSpeedTitle, 10) || 40;
    if (meterConfig.current['scrolling.speed.title'] != scrollSpeedTitle) {
        meterConfig.current['scrolling.speed.title'] = scrollSpeedTitle;
        noChanges = false;
    }
    
    // write scrolling speed album
    var scrollSpeedAlbum = parseInt(confData.scrollingSpeedAlbum, 10) || 40;
    if (meterConfig.current['scrolling.speed.album'] != scrollSpeedAlbum) {
        meterConfig.current['scrolling.speed.album'] = scrollSpeedAlbum;
        noChanges = false;
    }
    
    // save config file and restart meter if changes were made
    if (!noChanges) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
        // Restart meter to apply new scrolling settings
        if (fs.existsSync(runFlag)){fs.removeSync(runFlag);}
    }
  } else {
      self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
  }
  
  if (uiNeedsUpdate) {self.updateUIConfig();}
  setTimeout(function () {
    if (noChanges) {
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
    } else {
        self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    }
  }, 500);
}; // end saveScrollingConf -------------------------------------

// Animation settings save handler
//-------------------------------------------------------------

Glass.prototype.saveAnimationConf = function (confData) {
  const self = this;
  let noChanges = true;
  uiNeedsUpdate = false;
  
  if (fs.existsSync(MeterConfigFile)){

    // start/stop animation on/off (moved here from the global section). start.animation
    // is only meaningful with SDL2, matching the control's visibility in getUIConfig.
    if (use_SDL2) {
        var startAnimation = confData.animation ? 'True' : 'False';
        if (meterConfig.current['start.animation'] != startAnimation) {
            meterConfig.current['start.animation'] = startAnimation;
            noChanges = false;
        }
    }
    
    // write transition type
    var transitionType = confData.transitionType.value || 'fade';
    if (meterConfig.current['transition.type'] != transitionType) {
        meterConfig.current['transition.type'] = transitionType;
        noChanges = false;
    }
    
    // write transition duration
    if (Number.isNaN(parseFloat(confData.transitionDuration)) || !isFinite(confData.transitionDuration)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.TRANSITION_DURATION') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.transitionDuration = self.minmax('TRANSITION_DURATION', confData.transitionDuration, minmax[9], true);
        if (meterConfig.current['transition.duration'] != confData.transitionDuration) {
            meterConfig.current['transition.duration'] = confData.transitionDuration;
            noChanges = false;
        }
    }
    
    // write transition color
    var transitionColor = confData.transitionColor.value || 'black';
    if (meterConfig.current['transition.color'] != transitionColor) {
        meterConfig.current['transition.color'] = transitionColor;
        noChanges = false;
    }
    
    // write transition opacity
    if (Number.isNaN(parseInt(confData.transitionOpacity, 10)) || !isFinite(confData.transitionOpacity)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.TRANSITION_OPACITY') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.transitionOpacity = self.minmax('TRANSITION_OPACITY', confData.transitionOpacity, minmax[10]);
        if (meterConfig.current['transition.opacity'] != confData.transitionOpacity) {
            meterConfig.current['transition.opacity'] = confData.transitionOpacity;
            noChanges = false;
        }
    }
    
    if (!noChanges) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
        // Restart meter to apply new settings
        if (fs.existsSync(runFlag)){fs.removeSync(runFlag);}
    }
  } else {
      self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
  }
  
  if (uiNeedsUpdate) {self.updateUIConfig();}
  setTimeout(function () {
    if (noChanges) {
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
    } else {
        self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    }
  }, 500);
}; // end saveAnimationConf -------------------------------------

// Rotation settings save handler
//-------------------------------------------------------------

Glass.prototype.saveRotationConf = function (confData) {
  const self = this;
  let noChanges = true;
  uiNeedsUpdate = false;
  
  if (fs.existsSync(MeterConfigFile)){
    
    // write rotation quality
    var rotationQuality = confData.rotationQuality.value || 'medium';
    if (meterConfig.current['rotation.quality'] != rotationQuality) {
        meterConfig.current['rotation.quality'] = rotationQuality;
        noChanges = false;
    }
    
    // write reel direction
    var reelDirection = confData.reelDirection.value || 'ccw';
    if (meterConfig.current['reel.direction'] != reelDirection) {
        meterConfig.current['reel.direction'] = reelDirection;
        noChanges = false;
    }
    
    // write rotation FPS (custom mode)
    if (Number.isNaN(parseInt(confData.rotationFPS, 10)) || !isFinite(confData.rotationFPS)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.ROTATION_FPS') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.rotationFPS = self.minmax('ROTATION_FPS', confData.rotationFPS, minmax[11]);
        if (meterConfig.current['rotation.fps'] != confData.rotationFPS) {
            meterConfig.current['rotation.fps'] = confData.rotationFPS;
            noChanges = false;
        }
    }
    
    // write rotation speed (vinyl multiplier)
    if (Number.isNaN(parseFloat(confData.rotationSpeed)) || !isFinite(confData.rotationSpeed)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.ROTATION_SPEED') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.rotationSpeed = self.minmax('ROTATION_SPEED', confData.rotationSpeed, minmax[12], true);
        if (meterConfig.current['rotation.speed'] != confData.rotationSpeed) {
            meterConfig.current['rotation.speed'] = confData.rotationSpeed;
            noChanges = false;
        }
    }
    
    // write spool left speed (cassette multiplier)
    if (Number.isNaN(parseFloat(confData.spoolLeftSpeed)) || !isFinite(confData.spoolLeftSpeed)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.SPOOL_LEFT_SPEED') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.spoolLeftSpeed = self.minmax('SPOOL_LEFT_SPEED', confData.spoolLeftSpeed, minmax[13], true);
        if (meterConfig.current['spool.left.speed'] != confData.spoolLeftSpeed) {
            meterConfig.current['spool.left.speed'] = confData.spoolLeftSpeed;
            noChanges = false;
        }
    }
    
    // write spool right speed (cassette multiplier)
    if (Number.isNaN(parseFloat(confData.spoolRightSpeed)) || !isFinite(confData.spoolRightSpeed)) {
        uiNeedsUpdate = true;
        setTimeout(function () {
            self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.SPOOL_RIGHT_SPEED') + self.commandRouter.getI18nString('GLASS.NAN'));
        }, 500);
    } else {
        confData.spoolRightSpeed = self.minmax('SPOOL_RIGHT_SPEED', confData.spoolRightSpeed, minmax[14], true);
        if (meterConfig.current['spool.right.speed'] != confData.spoolRightSpeed) {
            meterConfig.current['spool.right.speed'] = confData.spoolRightSpeed;
            noChanges = false;
        }
    }
    
    // write spool adaptive (dynamic speeds based on progress)
    var spoolAdaptive = confData.spoolAdaptive || false;
    if (meterConfig.current['spool.adaptive'] != spoolAdaptive) {
        meterConfig.current['spool.adaptive'] = spoolAdaptive;
        noChanges = false;
    }
    
    if (!noChanges) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, {whitespace: true}));
        // Restart meter to apply new settings
        if (fs.existsSync(runFlag)){fs.removeSync(runFlag);}
    }
  } else {
      self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
  }
  
  if (uiNeedsUpdate) {self.updateUIConfig();}
  setTimeout(function () {
    if (noChanges) {
        self.commandRouter.pushToastMessage('info', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_CHANGES'));
    } else {
        self.commandRouter.pushToastMessage('success', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('COMMON.SETTINGS_SAVED_SUCCESSFULLY'));
    }
  }, 500);
}; // end saveRotationConf -------------------------------------

// Debug settings save handler
//-------------------------------------------------------------

Glass.prototype.updateConfigVersion = function () {
  const self = this;
  
  try {
    if (fs.existsSync(MeterConfigFile)) {
      var configContent = fs.readFileSync(MeterConfigFile, 'utf8');
      var newHash = crypto.createHash('md5').update(configContent).digest('hex').substring(0, 8);
      
      if (newHash !== remoteConfigVersion) {
        remoteConfigVersion = newHash;
        self.logger.info(id + 'Config version updated: ' + remoteConfigVersion);
      }
    }
  } catch (err) {
    self.logger.error(id + 'Failed to calculate config version: ' + err.message);
  }
  
  return remoteConfigVersion;
};

// Get current config version hash
Glass.prototype.getConfigVersion = function () {
  return remoteConfigVersion;
};

// HTTP endpoint: Return config.txt contents for remote clients
// Called via: GET /api/v1/pluginEndpoint?endpoint=glass&method=getRemoteConfig

function fanartArtistSlug(artist) {
  var raw = String(artist || '').trim().toLowerCase();
  if (!raw) {
    return '';
  }
  var ascii = raw.replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 80);
  if (ascii) {
    return ascii;
  }
  // Non-Latin names (Thai, CJK, Cyrillic, …): ASCII slug would be empty and
  // blocked fanart entirely; use a stable hashed cache directory instead.
  return 'u-' + crypto.createHash('sha256').update(raw, 'utf8').digest('hex').slice(0, 16);
}

function fanartDecodeUriPath(part) {
  if (!part || part.indexOf('%') === -1) {
    return part;
  }
  try {
    return decodeURIComponent(part);
  } catch (e) {
    return part;
  }
}

function fanartHttpsGetText(url, headers, timeoutMs) {
  return new Promise(function (resolve, reject) {
    var lib = url.indexOf('https') === 0 ? require('https') : require('http');
    var req = lib.get(url, { headers: headers || {} }, function (resp) {
      if (resp.statusCode >= 300 && resp.statusCode < 400 && resp.headers.location) {
        resp.resume();
        resolve(fanartHttpsGetText(resp.headers.location, headers, timeoutMs));
        return;
      }
      if (resp.statusCode !== 200) {
        resp.resume();
        reject(new Error('HTTP ' + resp.statusCode));
        return;
      }
      var data = '';
      resp.setEncoding('utf8');
      resp.on('data', function (c) { data += c; });
      resp.on('end', function () { resolve(data); });
    });
    req.on('error', reject);
    req.setTimeout(timeoutMs || 8000, function () { req.destroy(new Error('timeout')); });
  });
}

function fanartDownloadFile(url, dest, timeoutMs) {
  return new Promise(function (resolve, reject) {
    var lib = url.indexOf('https') === 0 ? require('https') : require('http');
    var req = lib.get(url, function (resp) {
      if (resp.statusCode >= 300 && resp.statusCode < 400 && resp.headers.location) {
        resp.resume();
        resolve(fanartDownloadFile(resp.headers.location, dest, timeoutMs));
        return;
      }
      if (resp.statusCode !== 200) {
        resp.resume();
        reject(new Error('HTTP ' + resp.statusCode));
        return;
      }
      var file = fs.createWriteStream(dest);
      resp.pipe(file);
      file.on('finish', function () { file.close(function () { resolve(true); }); });
      file.on('error', reject);
    });
    req.on('error', reject);
    req.setTimeout(timeoutMs || 15000, function () { req.destroy(new Error('timeout')); });
  });
}


Glass.prototype.fanartListLocalImages = function (dir) {
  var out = [];
  try {
    if (!dir || !fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) {
      return out;
    }
    fs.readdirSync(dir).forEach(function (f) {
      if (FANART_IMAGE_EXTS.indexOf(path.extname(f).toLowerCase()) !== -1) {
        var p = path.join(dir, f);
        try { if (fs.statSync(p).isFile()) out.push(p); } catch (e) {}
      }
    });
  } catch (e) {}
  out.sort();
  return out;
};

Glass.prototype.fanartSourceFingerprint = function (paths) {
  if (!paths || !paths.length) {
    return '';
  }
  return paths.map(function (p) {
    try {
      var st = fs.statSync(p);
      return p + ':' + st.mtimeMs + ':' + st.size;
    } catch (e) {
      return p + ':0:0';
    }
  }).sort().join(',');
};

Glass.prototype.fanartResolveLocalSource = function (artist, uri) {
  var self = this;
  if (artist.indexOf('/') === -1 && artist.indexOf('..') === -1) {
    var personalImgs = self.fanartListLocalImages(FanartPersonalArtDir + '/' + artist);
    if (personalImgs.length) {
      return {
        source: 'personal',
        picked: personalImgs.map(function (p) { return { type: 'file', path: p }; }),
        fingerprint: 'personal:' + self.fanartSourceFingerprint(personalImgs)
      };
    }
  }
  if (uri) {
    var artistDir = self.fanartResolveArtistMusicDir(uri);
    if (artistDir) {
      var subImgs = self.fanartListLocalImages(path.join(artistDir, 'fanart'));
      if (subImgs.length) {
        return {
          source: 'local-fanart',
          picked: subImgs.map(function (p) { return { type: 'file', path: p }; }),
          fingerprint: 'local-fanart:' + self.fanartSourceFingerprint(subImgs)
        };
      }
    }
  }
  return null;
};

Glass.prototype.fanartShouldUseDiskCache = function (cached, artist, uri) {
  var self = this;
  if (!cached || !cached.images || !cached.images.length) {
    return false;
  }
  var firstFile = PluginPath + '/' + cached.images[0].replace('user_interface/glass/', '');
  if (!fs.existsSync(firstFile)) {
    return false;
  }
  var localNow = self.fanartResolveLocalSource(artist, uri);
  if (localNow) {
    return cached.source === localNow.source && cached.sourceSig === localNow.fingerprint;
  }
  if (cached.source === 'personal' || cached.source === 'local-fanart') {
    return false;
  }
  return (Date.now() - (cached.ts || 0)) < FANART_TTL_MS;
};

Glass.prototype.fanartResolveArtistMusicDir = function (uri) {
  try {
    var san = fanartDecodeUriPath(uri.replace(/^music-library\/?/, '').replace(/^mnt\/?/, ''));
    var base = san.startsWith('/') ? '/mnt' + san : '/mnt/' + san;
    var albumDir = path.dirname(path.resolve(base));
    var artistDir = path.dirname(albumDir);
    if (artistDir.indexOf('/mnt/') !== 0 || artistDir.indexOf('..') !== -1) {
      return null;
    }
    return artistDir;
  } catch (e) {
    return null;
  }
};

Glass.prototype.fanartReadManifest = function (p) {
  try { if (fs.existsSync(p)) { return JSON.parse(fs.readFileSync(p, 'utf8')); } } catch (e) {}
  return null;
};

Glass.prototype.fanartWriteManifest = function (p, obj) {
  try { fs.writeFileSync(p, JSON.stringify(obj)); } catch (e) {}
};

// Resolve artist name -> MusicBrainz MBID, cached (incl. negative results). Never guesses
// on a low-confidence match; transient errors are not cached.
Glass.prototype.fanartResolveMBID = async function (artist) {
  var self = this;
  var key = artist.toLowerCase();
  var cacheFile = FanartCacheDir + '/mbid.json';
  var cache = {};
  try { if (fs.existsSync(cacheFile)) { cache = JSON.parse(fs.readFileSync(cacheFile, 'utf8')) || {}; } } catch (e) {}
  if (Object.prototype.hasOwnProperty.call(cache, key)) {
    return cache[key];
  }
  var mbid = null;
  try {
    var url = 'https://musicbrainz.org/ws/2/artist/?query=' + encodeURIComponent('artist:"' + artist + '"') + '&fmt=json&limit=1';
    var ua = 'Glass/' + pluginVersion + ' ( https://github.com/foonerd/glass )';
    var body = await fanartHttpsGetText(url, { 'User-Agent': ua, 'Accept': 'application/json' }, 8000);
    var json = JSON.parse(body);
    if (json && json.artists && json.artists.length && json.artists[0].id) {
      var top = json.artists[0];
      if (top.score === undefined || top.score >= 90) {
        mbid = top.id;
      }
    }
  } catch (e) {
    galleryLog(self.logger, 'verbose', 'fanart MBID lookup failed for ' + artist + ': ' + e.message);
    return null; // do not negative-cache transient failures
  }
  try { fs.ensureDirSync(FanartCacheDir); cache[key] = mbid; fs.writeFileSync(cacheFile, JSON.stringify(cache)); } catch (e) {}
  return mbid;
};

Glass.prototype.fanartFetchFanartTv = async function (mbid, apiKey) {
  var url = 'https://webservice.fanart.tv/v3/music/' + encodeURIComponent(mbid) + '?api_key=' + encodeURIComponent(apiKey || FANART_TV_PROJECT_KEY);
  var body = await fanartHttpsGetText(url, { 'Accept': 'application/json' }, 10000);
  var json = JSON.parse(body);
  var urls = [];
  if (json && Array.isArray(json.artistbackground)) {
    json.artistbackground.forEach(function (b) { if (b && b.url) { urls.push(b.url); } });
  }
  return urls;
};

Glass.prototype.fanartFetchMetaVolumio = async function (artist) {
  var variant = 'volumio';
  try {
    variant = execSync("cat /etc/os-release | grep ^VOLUMIO_VARIANT | tr -d 'VOLUMIO_VARIANT=\"'").toString().replace('\n', '').trim() || 'volumio';
  } catch (e) {}
  var url = 'https://meta.volumio.org/metas/v1/getDatas?mode=artistArt&artist=' + encodeURIComponent(artist.replace('&', 'and')) + '&variant=' + encodeURIComponent(variant);
  var body = await fanartHttpsGetText(url, { 'Accept': 'application/json' }, 8000);
  var json = JSON.parse(body);
  if (json && json.success && json.data) {
    if (typeof json.data === 'string') { return json.data; }
    if (Array.isArray(json.data) && json.data.length) { return json.data[0]; }
  }
  return null;
};

// HTTP endpoint: artist fanart slideshow source list (Item 6). Returns an ordered list
// of sectionimage paths (served via /albumart?sectionimage=...), populating an on-disk
// cache. Cascade: personal artist folder -> fanart/ subfolder -> fanart.tv (MBID) ->
// meta.volumio.org single. POST { endpoint: 'glass_artistfanart', data: { artist, uri } }
function fanartPlaybackOptions(self) {
  var fanartIntervalMs = (parseInt(self.config.get('fanartInterval'), 10) || 0) * 1000;
  var fanartTransition = self.config.get('fanartTransition') || 'none';
  if (['none', 'fade', 'merge'].indexOf(fanartTransition) === -1) { fanartTransition = 'none'; }
  var fanartTransitionMs = parseInt(self.config.get('fanartTransitionMs'), 10);
  if (isNaN(fanartTransitionMs) || fanartTransitionMs < 50) { fanartTransitionMs = 600; }
  var fanartOrder = self.config.get('fanartOrder') || 'sequential';
  if (['sequential', 'random'].indexOf(fanartOrder) === -1) { fanartOrder = 'sequential'; }
  return {
    interval_ms: fanartIntervalMs,
    transition: fanartTransition,
    transition_ms: fanartTransitionMs,
    order: fanartOrder
  };
}

Glass.prototype.getArtistFanart = async function (data) {
  var self = this;
  var artist = (data && typeof data.artist === 'string') ? data.artist.trim() : '';
  var uri = (data && typeof data.uri === 'string') ? data.uri.trim() : '';
  if (!artist) {
    return { success: false, error: 'no artist' };
  }
  // Master switch (Item 6): fanart only renders when globally enabled AND the skin
  // declares fanart slots. When disabled, return an empty set so the renderer clears.
  if (self.config.get('fanartEnabled') !== true) {
    return { success: true, source: 'disabled', images: [], interval_ms: 0, order: 'sequential' };
  }
  var fanartOpts = fanartPlaybackOptions(self);
  var slug = fanartArtistSlug(artist);
  if (!slug) {
    return { success: false, error: 'invalid artist' };
  }
  var artistCacheDir = FanartCacheDir + '/' + slug;
  var manifestPath = artistCacheDir + '/manifest.json';
  try {
    var cached = self.fanartReadManifest(manifestPath);
    if (self.fanartShouldUseDiskCache(cached, artist, uri)) {
      galleryLog(self.logger, 'basic', 'getArtistFanart cache hit ' + slug + ' (' + cached.images.length + ' img, ' + cached.source + ')');
      return { success: true, source: cached.source + ':cached', images: cached.images,
        interval_ms: fanartOpts.interval_ms, transition: fanartOpts.transition,
        transition_ms: fanartOpts.transition_ms, order: fanartOpts.order };
    }
    fs.ensureDirSync(artistCacheDir);

    var source = null;
    var picked = [];
    var sourceSig = null;

    var localSource = self.fanartResolveLocalSource(artist, uri);
    if (localSource) {
      source = localSource.source;
      picked = localSource.picked;
      sourceSig = localSource.fingerprint;
    }

    // Tier 3: fanart.tv full set (MBID via MusicBrainz). Key mode decides which
    // api_key is used: 'project' = built-in key (testing/development only),
    // 'personal' = the listener's own fanart.tv key (required; skipped if blank).
    if (!picked.length) {
      var keyMode = self.config.get('fanartKeyMode') || 'personal';
      var apiKey = '';
      if (keyMode === 'project') {
        apiKey = FANART_TV_PROJECT_KEY;
      } else {
        try { apiKey = (self.config.get('fanart_personal_key') || '').trim(); } catch (e) {}
      }
      if (apiKey) {
        var mbid = await self.fanartResolveMBID(artist);
        if (mbid) {
          var urls = await self.fanartFetchFanartTv(mbid, apiKey);
          if (urls.length) {
            source = 'fanart.tv';
            picked = urls.map(function (u) { return { type: 'url', url: u }; });
          }
        }
      } else {
        galleryLog(self.logger, 'verbose', 'fanart.tv skipped: personal key mode with no key set');
      }
    }

    // Tier 4: meta.volumio.org single image
    if (!picked.length) {
      var metaUrl = await self.fanartFetchMetaVolumio(artist);
      if (metaUrl) {
        source = 'meta.volumio';
        picked = [{ type: 'url', url: metaUrl }];
      }
    }

    if (!picked.length) {
      self.fanartWriteManifest(manifestPath, { ts: Date.now(), source: 'none', images: [] });
      galleryLog(self.logger, 'basic', 'getArtistFanart: no fanart for "' + artist + '"');
      return { success: false, error: 'no fanart' };
    }

    // Default: cap at FANART_MAX_IMAGES. Unlimited mode skips the cap (crash risk).
    if (self.config.get('fanartUnlimitedImages') !== true && picked.length > FANART_MAX_IMAGES) {
      picked = picked.slice(0, FANART_MAX_IMAGES);
    }

    // Clear previous cached images for this artist (keep manifest)
    try {
      fs.readdirSync(artistCacheDir).forEach(function (f) {
        if (f !== 'manifest.json') { fs.removeSync(path.join(artistCacheDir, f)); }
      });
    } catch (e) {}

    var images = [];
    for (var i = 0; i < picked.length; i++) {
      var item = picked[i];
      var ext = '.jpg';
      try {
        if (item.type === 'file') {
          ext = path.extname(item.path).toLowerCase() || '.jpg';
        } else {
          ext = path.extname(item.url.split('?')[0]).toLowerCase();
          if (FANART_IMAGE_EXTS.indexOf(ext) === -1) { ext = '.jpg'; }
        }
        var destName = i + ext;
        var destPath = path.join(artistCacheDir, destName);
        if (item.type === 'file') {
          fs.copySync(item.path, destPath);
        } else {
          await fanartDownloadFile(item.url, destPath, 15000);
        }
        images.push(FanartSectionPrefix + slug + '/' + destName);
      } catch (e) {
        galleryLog(self.logger, 'verbose', 'fanart image ' + i + ' failed: ' + e.message);
      }
    }

    if (!images.length) {
      self.fanartWriteManifest(manifestPath, { ts: Date.now(), source: 'none', images: [] });
      return { success: false, error: 'fetch failed' };
    }

    self.fanartWriteManifest(manifestPath, { ts: Date.now(), source: source, sourceSig: sourceSig, images: images });
    galleryLog(self.logger, 'basic', 'getArtistFanart "' + artist + '" -> ' + images.length + ' image(s) from ' + source);
    return { success: true, source: source, images: images,
      interval_ms: fanartOpts.interval_ms, transition: fanartOpts.transition,
      transition_ms: fanartOpts.transition_ms, order: fanartOpts.order };
  } catch (err) {
    self.logger.error(id + 'getArtistFanart: ' + err.message);
    return { success: false, error: err.message };
  }
};

Glass.prototype.clearFanartCache = function () {
  var self = this;
  var defer = libQ.defer();
  var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
  self.commandRouter.broadcastMessage('openModal', {
    title: self.commandRouter.getI18nString('GLASS.CLEAR_FANART_CACHE_CONFIRM_TITLE'),
    message: self.commandRouter.getI18nString('GLASS.CLEAR_FANART_CACHE_CONFIRM_MSG'),
    size: 'md',
    buttons: [
      {
        name: self.commandRouter.getI18nString('COMMON.CANCEL'),
        class: 'btn btn-default',
        emit: 'closeModals',
        payload: ''
      },
      {
        name: self.commandRouter.getI18nString('GLASS.CLEAR_FANART_CACHE'),
        class: 'btn btn-warning',
        emit: 'callMethod',
        payload: {
          endpoint: 'user_interface/glass',
          method: 'clearFanartCacheConfirmed',
          data: {}
        }
      }
    ]
  });
  defer.resolve();
  return defer.promise;
};

/** Clear cached fanart images (keep mbid.json). Returns true on success. */
Glass.prototype._clearFanartImageCache = function () {
  var self = this;
  if (!fs.existsSync(FanartCacheDir)) {
    return true;
  }
  fs.readdirSync(FanartCacheDir).forEach(function (f) {
    if (f === 'mbid.json') { return; }
    fs.removeSync(path.join(FanartCacheDir, f));
  });
  galleryLog(self.logger, 'basic', 'fanart cache cleared (mbid.json preserved)');
  return true;
};

Glass.prototype.clearFanartCacheConfirmed = function () {
  var self = this;
  var defer = libQ.defer();
  var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
  try {
    self._clearFanartImageCache();
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    self.commandRouter.pushToastMessage('success', pluginName, self.commandRouter.getI18nString('GLASS.CLEAR_FANART_CACHE_DONE'));
  } catch (e) {
    self.logger.error(id + 'clearFanartCacheConfirmed: ' + e.message);
    self.commandRouter.pushToastMessage('error', pluginName, e.message);
  }
  defer.resolve();
  return defer.promise;
};

function escapeThemeGalleryHtml(text) {
  return String(text)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}


function escapeThemeGalleryJsString(text) {
  return JSON.stringify(String(text));
}


Glass.prototype.applyThemeTag = function (state) {
  var self = this;
  if (!state) state = self._themeTagState;
  if (!state || !meterConfig || !meterConfig.current) return;
  self._themeTagState = { album: state.album || '', uri: state.uri || '' };
  var rulesText = '';
  try { rulesText = self.config.get('themeTagRules') || ''; } catch (e) { return; }
  var picked = themeFolderForEdition(self._themeTagState.album, self._themeTagState.uri, rulesText);
  if (picked === null) {
    self.themeTagOverride = false;
    return;
  }
  var home = self.config.get('activeFolder') || '';
  var target = picked || home;
  if (!target) return;
  if (picked) {
    var pickedPath = base_folder_P + picked;
    var pickedOk = false;
    try { pickedOk = fs.existsSync(pickedPath) && fs.statSync(pickedPath).isDirectory(); } catch (e) {}
    if (!pickedOk) {
      self.logger.info(id + 'theme tag folder missing: ' + picked);
      target = home;
    }
  }
  if (!target || target === meterConfig.current[meterFolderStr]) {
    self.themeTagOverride = !!(picked && target === picked);
    return;
  }
  var result = self.applyActiveThemeFolder(target, { keepHome: true, allowBuiltin: true });
  if (result && result.changed) {
    self.logger.info(id + 'theme tag -> ' + target);
  }
  self.themeTagOverride = !!(picked && target === picked);
};

Glass.prototype.applyActiveThemeFolder = function (folder, opts) {
  var self = this;
  opts = opts || {};

  galleryLog(self.logger, 'basic', 'applyActiveThemeFolder called folder=' + folder);

  if (!folder || folder.indexOf('/') !== -1 || folder.indexOf('..') !== -1 || (!opts.allowBuiltin && folder.indexOf('_') === -1)) {
    galleryLog(self.logger, 'verbose', 'applyActiveThemeFolder rejected invalid folder');
    return { changed: false, error: 'invalid' };
  }

  var folderPath = base_folder_P + folder;
  if (!fs.existsSync(folderPath) || !fs.statSync(folderPath).isDirectory()) {
    return { changed: false, error: 'not_found' };
  }

  if (!fs.existsSync(MeterConfigFile)) {
    return { changed: false, error: 'no_config' };
  }

  if (!spectrum_config && fs.existsSync(SpectrumConfigFile)) {
    spectrum_config = ini.parse(fs.readFileSync(SpectrumConfigFile, 'utf-8'));
  }

  if (meterConfig.current[meterFolderStr] === folder) {
    galleryLog(self.logger, 'basic', 'applyActiveThemeFolder unchanged (already active)');
    return { changed: false };
  }

  var partFile = folder.split('_');
  var upperc = /\b([^-])/g;
  var str_empty = fs.existsSync(folderPath + '/meters.txt') ? '' : ' (empty)';
  var folderTitle = folder;
  if (partFile[1]) {
    folderTitle = (partFile[1]).replace(upperc, function (c) { return c.toUpperCase(); }) + '-' + partFile[2] + ' ' + partFile[0] + str_empty;
  }

  meterConfig.current[meterFolderStr] = folder;
  if (spectrum_config) {
    spectrum_config.current[SpectrumFolderStr] = folder;
  }
  if (!opts.keepHome) {
    self.config.set('activeFolder', folder);
    self.config.set('activeFolder_title', folderTitle);
  }
  meterConfig.current.meter = 'random';
  self.config.set('randomSelection', '');
  self.checkMetersFile();

  var dimensions = { width: '', height: '' };
  try {
    var files = fs.readdirSync(folderPath);
    files.forEach(function (file) {
      if (file.indexOf('-ext.') >= 0) {
        dimensions = sizeOf(folderPath + '/' + file);
      }
    });
  } catch (e) {
    self.logger.warn(id + 'applyActiveThemeFolder: could not read dimensions: ' + e.message);
  }
  meterConfig.current['screen.width'] = dimensions.width;
  meterConfig.current['screen.height'] = dimensions.height;

  fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
  if (spectrum_config) {
    fs.writeFileSync(SpectrumConfigFile, ini.stringify(spectrum_config, { whitespace: true }));
  }
  try { self.updateConfigVersion(); } catch (e) {}
  if (fs.existsSync(runFlag)) {
    fs.removeSync(runFlag);
  }

  if (!opts.keepHome) {
    uiNeedsUpdate = true;
    self.updateUIConfig();
  }

  galleryLog(self.logger, 'basic', 'applyActiveThemeFolder applied ' + folder + ' -> ' + folderTitle);
  return { changed: true, label: folderTitle };
};


Glass.prototype.removeThemeFolder = function (data) {
  var self = this;
  var defer = libQ.defer();
  var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
  var folder = (data && data.themeToRemove && typeof data.themeToRemove === 'object')
    ? data.themeToRemove.value
    : (data && data.themeToRemove);

  if (!self.isValidThemeFolderName(folder)) {
    self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.THEME_REMOVE_INVALID'));
    defer.resolve();
    return defer.promise;
  }

  var title = self.commandRouter.getI18nString('GLASS.THEME_REMOVE_CONFIRM_TITLE');
  var msg = self.commandRouter.getI18nString('GLASS.THEME_REMOVE_CONFIRM_MSG') + ' ' + folder;
  self.commandRouter.broadcastMessage('openModal', {
    title: title,
    message: msg,
    size: 'lg',
    buttons: [
      {
        name: self.commandRouter.getI18nString('COMMON.CANCEL'),
        class: 'btn btn-default',
        emit: 'closeModals',
        payload: ''
      },
      {
        name: self.commandRouter.getI18nString('GLASS.THEME_REMOVE_BTN'),
        class: 'btn btn-warning',
        emit: 'callMethod',
        payload: {
          endpoint: 'user_interface/glass',
          method: 'removeThemeFolderConfirmed',
          data: { folder: folder }
        }
      }
    ]
  });

  defer.resolve();
  return defer.promise;
};

Glass.prototype.isValidThemeFolderName = function (folder) {
  return !!folder && typeof folder === 'string' &&
    folder.indexOf('/') === -1 && folder.indexOf('..') === -1 && folder.indexOf('_') !== -1;
};

// Safely remove base+folder only when it resolves under the expected templates root.
Glass.prototype.removeThemeTreeFolder = function (baseDir, folder) {
  var self = this;
  if (!baseDir) {
    return false;
  }
  var root = path.resolve(baseDir);
  var target = path.resolve(path.join(root, folder));
  if (target.indexOf(root + path.sep) !== 0) {
    self.logger.warn(id + 'removeThemeTreeFolder: refusing path outside root: ' + target);
    return false;
  }
  if (fs.existsSync(target) && fs.statSync(target).isDirectory()) {
    fs.removeSync(target);
    return true;
  }
  return false;
};

Glass.prototype.removeThemeFolderConfirmed = function (data) {
  var self = this;
  var defer = libQ.defer();
  var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
  var folder = data && data.folder;

  self.commandRouter.closeModals();

  if (!self.isValidThemeFolderName(folder)) {
    self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.THEME_REMOVE_INVALID'));
    defer.resolve();
    return defer.promise;
  }

  // Reload configs so base paths and active folder are current
  try {
    if (fs.existsSync(MeterConfigFile)) {
      meterConfig = ini.parse(fs.readFileSync(MeterConfigFile, 'utf-8'));
      base_folder_P = meterConfig.current['base.folder'] + '/';
      if (base_folder_P === '/') {
        base_folder_P = DATA_DIR + '/templates/';
      }
    }
    if (fs.existsSync(SpectrumConfigFile)) {
      spectrum_config = ini.parse(fs.readFileSync(SpectrumConfigFile, 'utf-8'));
      base_folder_S = spectrum_config.current['base.folder'] + '/';
      if (base_folder_S === '/') {
        base_folder_S = DATA_DIR + '/templates_spectrum/';
      }
    }
  } catch (e) {
    self.logger.error(id + 'removeThemeFolderConfirmed: failed to reload config: ' + e.message);
    self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.THEME_REMOVE_INVALID'));
    defer.resolve();
    return defer.promise;
  }

  if (!fs.existsSync(base_folder_P + folder)) {
    self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.THEME_REMOVE_INVALID'));
    defer.resolve();
    return defer.promise;
  }

  // Enumerate remaining meter theme folders to guard the last-skin case
  var allFolders = [];
  try {
    fs.readdirSync(base_folder_P).forEach(function (f) {
      if (f.indexOf('_') !== -1 && fs.statSync(base_folder_P + f).isDirectory()) {
        allFolders.push(f);
      }
    });
  } catch (e) {
    self.logger.error(id + 'removeThemeFolderConfirmed: enumerate failed: ' + e.message);
  }
  var remaining = allFolders.filter(function (f) { return f !== folder; });
  if (remaining.length === 0) {
    self.commandRouter.pushToastMessage('warning', pluginName, self.commandRouter.getI18nString('GLASS.THEME_REMOVE_LAST'));
    defer.resolve();
    return defer.promise;
  }

  // If removing the active theme, switch to another one first so configs stay valid
  var wasActive = (meterConfig.current[meterFolderStr] === folder);
  var switchedTo = null;
  if (wasActive) {
    switchedTo = remaining[0];
    self.applyActiveThemeFolder(switchedTo);
  }

  // Delete from both trees (meters + spectrum); spectrum twin is optional
  self.removeThemeTreeFolder(base_folder_P, folder);
  self.removeThemeTreeFolder(base_folder_S, folder);

  // Drop the cached gallery preview for the removed folder, if present
  try {
    if (fs.existsSync(ThemeGalleryDir)) {
      fs.readdirSync(ThemeGalleryDir).forEach(function (f) {
        var cacheExt = path.extname(f).toLowerCase();
        var cacheBase = cacheExt ? f.slice(0, -cacheExt.length) : f;
        if (f === folder + '.select.html' || (cacheBase === folder && THEME_GALLERY_CACHE_EXTS.indexOf(cacheExt) !== -1)) {
          fs.removeSync(ThemeGalleryDir + '/' + f);
        }
      });
    }
  } catch (e) {
    galleryLog(self.logger, 'verbose', 'removeThemeFolderConfirmed: cache cleanup failed: ' + e.message);
  }

  galleryLog(self.logger, 'basic', 'removeThemeFolderConfirmed removed ' + folder + (wasActive ? ' (was active -> ' + switchedTo + ')' : ''));

  if (wasActive) {
    self.commandRouter.pushToastMessage('success', pluginName,
      self.commandRouter.getI18nString('GLASS.THEME_REMOVE_ACTIVE_RESET') + ' ' + switchedTo);
  } else {
    self.commandRouter.pushToastMessage('success', pluginName,
      self.commandRouter.getI18nString('GLASS.THEME_REMOVE_DONE') + ' ' + folder);
  }

  uiNeedsUpdate = true;
  self.updateUIConfig();

  defer.resolve();
  return defer.promise;
};


Glass.prototype.minmax = function (item, value, attrib, isFloat) {
  var self = this;
  var num = isFloat ? parseFloat(value) : parseInt(value, 10);
  if (Number.isNaN(num) || !isFinite(value)) {
      uiNeedsUpdate = true;
      return attrib[2];
  }
    if (num < attrib[0]) {
        setTimeout(function () {
            self.commandRouter.pushToastMessage("info", self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.' + item.toUpperCase()) + ': ' + self.commandRouter.getI18nString('GLASS.INFO_MIN'));
        }, 700);        
        uiNeedsUpdate = true;
        return attrib[0];
    }
    if (num > attrib[1]) {
        setTimeout(function () {
            self.commandRouter.pushToastMessage("info", self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.' + item.toUpperCase()) + ': ' + self.commandRouter.getI18nString('GLASS.INFO_MAX'));
        }, 700); 
        uiNeedsUpdate = true;
        return attrib[1];
    }
    return num;
};

Glass.prototype.updateUIConfig = function () {
  const self = this;
  const defer = libQ.defer();

  self.commandRouter.getUIConfigOnPlugin('user_interface', 'glass', {})
    .then(function (uiconf) {
      self.commandRouter.broadcastMessage('pushUiConfig', uiconf);
    });
  self.commandRouter.broadcastMessage('pushUiConfig');
  uiNeedsUpdate = false;
  return defer.promise;
};

// Normalize template folder permissions for SMB share access
// When enabled: dirs 777, files 666 (writable by SMB nobody:nogroup)
// When disabled: dirs 755, files 644 (standard permissions)


Glass.prototype.checkMetersFile = function (){
    const self = this;
    const defer = libQ.defer();
    var meters_file = base_folder_P + meterConfig.current[meterFolderStr] + '/meters.txt';
  
    if (!fs.existsSync(meters_file)){
        setTimeout(function () {
            self.commandRouter.pushToastMessage('warning', self.commandRouter.getI18nString('GLASS.NOMETERSWARNING_TITLE'), self.commandRouter.getI18nString('GLASS.NOMETERSWARNING'));
        }, 1500);
    }

    return defer.promise;
};

Glass.prototype.checkListMode = function (listStr){
    const self = this;
	
	var meters_file = base_folder_P + meterConfig.current[meterFolderStr] + '/meters.txt';
	var meterSectArray = [];
    var listError = [];
	var listArray = (listStr).split(',');
	var not_found = false;
	
	if (fs.existsSync(meters_file)){
	    var metersconfig = ini.parse(fs.readFileSync(meters_file, 'utf-8'));
		
		// get sections from file	
		for (var section in metersconfig) {
			meterSectArray.push(section);
		}
		// check if list entry in section
		for (var i in listArray) {
			if (!meterSectArray.includes(listArray[i].trim())) {
                listError.push(listArray[i]);
                not_found = true;
            }
		}
	
	} else {
		not_found = true;
	}
  
    if (not_found){
        setTimeout(function () {
        // create a hint as modal
        var responseData = {
        title: self.commandRouter.getI18nString('GLASS.NOTINLIST_TITLE'),
        message: self.commandRouter.getI18nString('GLASS.NOTINLIST') + listError,
        size: 'lg',
        buttons: [
            {
            name: self.commandRouter.getI18nString('COMMON.GOT_IT'),
            class: 'btn btn-info ng-scope',
            emit: '',
            payload: ''
            }
        ]
        };
        self.commandRouter.broadcastMessage('openModal', responseData);
        }, 1000);
		return false;
    }

    return true;
};



// check, if MPD output enabled


// The ALSA contribution: the template of the architecture, written where
// Volumio collects it. The tap heads it on every path.
Glass.prototype.writeAsoundConfigModular = function () {
    var self = this;
    var defer = libQ.defer();
    var isX64 = self.volumioArch() === 'x64';
    var asoundTmpl = __dirname + (isX64 ? '/Glass.postGlass.5.x64.conf' : asound) + '.tmpl';
    var asoundConf = __dirname + '/asound' + asound;
    if (!fs.existsSync(asoundTmpl)) {
        self.logger.error(id + 'ALSA template missing: ' + asoundTmpl);
        defer.resolve();
        return defer.promise;
    }
    fs.writeFile(asoundConf, fs.readFileSync(asoundTmpl, 'utf8'), 'utf8', function (err) {
        if (err) {
            self.logger.error(id + 'cannot write ' + asoundConf + ': ' + err);
        } else {
            alsaLog(self.logger, 'basic', 'config written: ' + asoundConf);
        }
        defer.resolve();
    });
    return defer.promise;
};

// What earlier releases put beside the tap, taken back once: the MPD side
// output and its include, the copies mounted over MPD's and AirPlay's
// configuration templates, Spotify's own PCM, and Soloist's metering
// device. The tap meters every source on the main path, so all of them
// play to `volumio` again. `force` does it again, for the uninstall.
Glass.prototype.retireSideOutputs = function (force) {
    var self = this;
    if (!force && self.config.get('sideOutputsRetired') === true) {
        return libQ.resolve();
    }
    var chain = libQ.resolve();
    var mpdChanged = false;
    if (fs.existsSync(MPD_include)) {
        try { fs.removeSync(MPD_include); mpdChanged = true; } catch (e) {}
    }
    if (fs.existsSync(MPD)) {
        chain = chain
            .then(function () { return self.unmount_tmpl(MPDtmpl); })
            .then(function () {
                try { fs.removeSync(MPD); } catch (e) {}
                mpdChanged = true;
            });
    }
    chain = chain.then(function () {
        if (mpdChanged) {
            self.logger.info(id + 'the MPD side output of an earlier release is retired');
            return self.recreate_mpdconf().then(self.restartMpd.bind(self));
        }
    });
    if (fs.existsSync(spotify_config)) {
        try {
            var spotifydata = fs.readFileSync(spotify_config, 'utf8');
            if (spotifydata.indexOf('spotify') !== -1) {
                fs.writeFileSync(spotify_config, spotifydata.replace('spotify', 'volumio'), 'utf8');
                if (self.getPluginStatus('music_service', 'spop') === 'STARTED') {
                    self.commandRouter.executeOnPlugin('music_service', 'spop', 'initializeLibrespotDaemon', '');
                }
                self.logger.info(id + 'Spotify plays to volumio again');
            }
        } catch (e) {
            self.logger.warn(id + 'spotify: ' + (e && e.message ? e.message : e));
        }
    }
    if (fs.existsSync(AIR)) {
        chain = chain
            .then(function () { return self.unmount_tmpl(AIRtmpl); })
            .then(function () {
                try { fs.removeSync(AIR); } catch (e) {}
                if (self.getPluginStatus('music_service', 'airplay_emulation') === 'STARTED') {
                    self.commandRouter.executeOnPlugin('music_service', 'airplay_emulation', 'startShairportSync', '');
                }
                self.logger.info(id + 'AirPlay plays to volumio again');
            });
    }
    if (self.getPluginStatus('music_service', 'soloist_connect') === 'STARTED') {
        try {
            self.commandRouter.executeOnPlugin('music_service', 'soloist_connect', 'setPeppyMetering', false);
        } catch (e) {}
    }
    return chain.then(function () {
        self.config.set('sideOutputsRetired', true);
    });
};

//mount a copy of changed file over 
Glass.prototype.mount_tmpl = function (data_source, data_dest) {
  var self = this;
  var defer = libQ.defer();
  
  exec('/bin/df ' + data_dest + ' | /bin/grep ' + data_dest + ' && /bin/echo || /bin/echo volumio | /usr/bin/sudo -S /bin/mount --bind ' + data_source + ' ' + data_dest, function (error, stdout, stderr) {        
    if (error) {
        self.logger.error(id + 'Error mount ' + data_source + ' ' + error);
    } else {
        defer.resolve();
    }    
  });        
  
  return defer.promise;
};

//unmount a copy of changed file
Glass.prototype.unmount_tmpl = function (data_dest) {
  var self = this;
  var defer = libQ.defer();

  // Nothing mounted there is not an error.
  exec('/bin/mountpoint -q ' + data_dest + ' && /bin/echo volumio | /usr/bin/sudo -S /bin/umount ' + data_dest + ' || /bin/true', { uid: 1000, gid: 1000 }, function (error, stdout, stderr) {
    if (error) {
        self.logger.error(id + 'Error unmount ' + data_dest + ' ' + error);
    }
    defer.resolve(); // resolve anyway to not block chain
  });
  
  return defer.promise;
};

// restart MPD-deamon
Glass.prototype.restartMpd = function () {
  var self = this;
  var defer = libQ.defer();

  setTimeout(function () {
    self.commandRouter.executeOnPlugin('music_service', 'mpd', 'restartMpd', '');
    defer.resolve();
  }, 500);

  return defer.promise;
};


// recreate active /etc/mpd.conf
Glass.prototype.recreate_mpdconf = function () {
  const self = this;
  let defer = libQ.defer();
  
  self.commandRouter.executeOnPlugin('music_service', 'mpd', 'createMPDFile', function(error) {
    if (error) {
        self.logger.error(id + 'Cannot create /etc/mpd.conf ' + error);
    } else {
        defer.resolve();
    }
  });
  return defer.promise;
};


Glass.prototype.updateALSAConfigFile = function () {
	var self = this;
    var defer = libQ.defer();
    var done = false;
    var finish = function () {
        if (done) return;
        done = true;
        defer.resolve();
    };
    var ret;
    try {
        ret = self.commandRouter.executeOnPlugin('audio_interface', 'alsa_controller', 'updateALSAConfigFile');
    } catch (e) {
        self.logger.error(id + 'updateALSAConfigFile: ' + e);
        finish();
        return defer.promise;
    }
    if (ret && typeof ret.then === 'function') {
        ret.then(finish).fail(finish);
    } else {
        finish();
    }
    return defer.promise;
};
    
//--------------------------------------------------------------

// called from commandrouter to find the language file
Glass.prototype.getI18nFile = function (langCode) {
  const i18nFiles = fs.readdirSync(path.join(__dirname, 'i18n'));
  const langFile = 'strings_' + langCode + '.json';

  // check for i18n file fitting the system language
  if (i18nFiles.some(function (i18nFile) { return i18nFile === langFile; })) {
    return path.join(__dirname, 'i18n', langFile);
  }
  // return default i18n file
  return path.join(__dirname, 'i18n', 'strings_en.json');
};

Glass.prototype.getConfigParam = function (key) {
  var self = this;
  return self.config.get(key);
};

Glass.prototype.setConfigParam = function (data) {
  var self = this;
  self.config.set(data.key, data.value);
};

Glass.prototype.getPluginStatus = function (category, name) {
  var self = this;
  
  var PlugInConfig = new (require('v-conf'))();
  PlugInConfig.loadFile(PluginConfiguration);
  var retStr = PlugInConfig.get(category + '.' + name + '.status');
  retStr = typeof retStr === 'undefined' ? 'null' : retStr;
  return retStr;  
};


//-------------------------------------------------------------

// Continuity Engine - backup and restore helper methods
// -------------------------------------------------------------

// List all valid backups under BackupsPath.
// Returns array of {name, createdMs, createdLabel, pluginVersion, path}
// sorted newest first. Entries without a valid manifest.json are skipped.

Glass.prototype.listSettingsBackups = function () {
    var self = this;
    var results = [];
    
    try {
        if (!fs.existsSync(BackupsPath)) {
            return results;
        }
        
        var entries = fs.readdirSync(BackupsPath);
        entries.forEach(function (entry) {
            var entryPath = BackupsPath + '/' + entry;
            var manifestPath = entryPath + '/' + BackupManifestName;
            
            try {
                if (!fs.statSync(entryPath).isDirectory()) return;
                if (!fs.existsSync(manifestPath)) return;
                
                var manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
                if (!manifest || typeof manifest !== 'object') return;
                if (manifest.schema_version === undefined) return;
                
                var createdMs = 0;
                var createdLabel = '';
                if (manifest.created) {
                    var d = new Date(manifest.created);
                    createdMs = d.getTime();
                    if (!isNaN(createdMs)) {
                        var pad = function (n) { return n < 10 ? '0' + n : '' + n; };
                        createdLabel = d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes());
                    }
                }
                
                results.push({
                    name: entry,
                    createdMs: createdMs,
                    createdLabel: createdLabel || '?',
                    pluginVersion: manifest.plugin_version || '?',
                    path: entryPath
                });
            } catch (e) {
                self.logger.warn(id + 'listSettingsBackups: skipping invalid entry ' + entry + ': ' + e.message);
            }
        });
    } catch (e) {
        self.logger.error(id + 'listSettingsBackups: ' + e.message);
    }
    
    results.sort(function (a, b) { return b.createdMs - a.createdMs; });
    return results;
};

// Create a new backup with the name typed in the UI.
// Copies config.json, peppymeter config.txt and spectrum config.txt into
// a named subdirectory of BackupsPath and writes a manifest.json.
Glass.prototype.createSettingsBackup = function (data) {
    var self = this;
    var defer = libQ.defer();
    
    try {
        var rawName = (data && data.backupName !== undefined) ? String(data.backupName) : '';
        var backupName = rawName.trim();
        
        if (!backupName) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NAME_REQUIRED'));
            defer.resolve();
            return defer.promise;
        }
        if (!BackupNameRegex.test(backupName) || backupName.indexOf('..') !== -1) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NAME_INVALID'));
            defer.resolve();
            return defer.promise;
        }
        
        if (!fs.existsSync(BackupsPath)) {
            fs.mkdirSync(BackupsPath, { recursive: true });
        }
        
        var targetDir = BackupsPath + '/' + backupName;
        
        // Collision: reject with toast, do not overwrite
        if (fs.existsSync(targetDir)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NAME_EXISTS'));
            defer.resolve();
            return defer.promise;
        }
        
        // Disk-space guard. Skipped silently if statfsSync is unavailable
        // for any reason (older filesystems, mount quirks, etc).
        try {
            if (typeof fs.statfsSync === 'function') {
                var stats = fs.statfsSync(BackupsPath);
                var freeBytes = stats.bavail * stats.bsize;
                if (freeBytes < BackupMinFreeBytes) {
                    self.commandRouter.pushToastMessage('error',
                        self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                        self.commandRouter.getI18nString('GLASS.BACKUP_DISK_FULL'));
                    defer.resolve();
                    return defer.promise;
                }
            }
        } catch (e) {
            self.logger.warn(id + 'createSettingsBackup: statfsSync failed, skipping disk check: ' + e.message);
        }
        
        // Non-blocking warning for clutter. Create still proceeds.
        var existingList = self.listSettingsBackups();
        if (existingList.length >= BackupWarnCount) {
            self.commandRouter.pushToastMessage('warning',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_COUNT_WARN'));
        }
        
        // Verify source files exist before we create any destination files
        var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
        if (!fs.existsSync(configFile)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_SOURCE_MISSING') + ': config.json');
            defer.resolve();
            return defer.promise;
        }
        if (!fs.existsSync(MeterConfigFile)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_SOURCE_MISSING') + ': peppymeter config.txt');
            defer.resolve();
            return defer.promise;
        }
        if (!fs.existsSync(SpectrumConfigFile)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_SOURCE_MISSING') + ': spectrum config.txt');
            defer.resolve();
            return defer.promise;
        }
        
        fs.mkdirSync(targetDir, { recursive: true });
        fs.copySync(configFile, targetDir + '/config.json');
        fs.copySync(MeterConfigFile, targetDir + '/' + PeppyConfBackupName);
        fs.copySync(SpectrumConfigFile, targetDir + '/' + SpectrumConfBackupName);
        
        var manifest = {
            schema_version: BackupSchemaVersion,
            plugin_version: pluginVersion,
            created: new Date().toISOString(),
            name: backupName,
            files: ['config.json', PeppyConfBackupName, SpectrumConfBackupName]
        };
        fs.writeFileSync(targetDir + '/' + BackupManifestName, JSON.stringify(manifest, null, 2));
        
        self.logger.info(id + 'createSettingsBackup: created backup "' + backupName + '"');
        self.commandRouter.pushToastMessage('success',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_CREATED'));
        
        self.updateUIConfig();
        defer.resolve();
    } catch (e) {
        self.logger.error(id + 'createSettingsBackup: ' + e.message);
        self.commandRouter.pushToastMessage('error',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_CREATE_FAILED'));
        defer.resolve();
    }
    
    return defer.promise;
};

// Restore a backup by name. Validates manifest and parses all files
// before overwriting anything, so a corrupt backup cannot damage the
// current live config.
Glass.prototype.restoreSettingsBackup = function (data) {
    var self = this;
    var defer = libQ.defer();
    
    try {
        var backupName = '';
        if (data && data.selectedBackup) {
            backupName = (typeof data.selectedBackup === 'object') ? data.selectedBackup.value : String(data.selectedBackup);
        }
        backupName = (backupName || '').trim();
        
        if (!backupName) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NOT_SELECTED'));
            defer.resolve();
            return defer.promise;
        }
        
        // Path-traversal guard
        if (!BackupNameRegex.test(backupName) || backupName.indexOf('..') !== -1 || backupName.indexOf('/') !== -1 || backupName.indexOf('\\') !== -1) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NAME_INVALID'));
            defer.resolve();
            return defer.promise;
        }
        
        var sourceDir = BackupsPath + '/' + backupName;
        var manifestPath = sourceDir + '/' + BackupManifestName;
        
        if (!fs.existsSync(sourceDir) || !fs.existsSync(manifestPath)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NOT_FOUND'));
            defer.resolve();
            return defer.promise;
        }
        
        var manifest;
        try {
            manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
        } catch (e) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_MANIFEST_INVALID'));
            defer.resolve();
            return defer.promise;
        }
        
        if (!manifest || typeof manifest !== 'object' || manifest.schema_version === undefined) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_MANIFEST_INVALID'));
            defer.resolve();
            return defer.promise;
        }
        
        if (manifest.schema_version > BackupSchemaVersion) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_SCHEMA_UNSUPPORTED'));
            defer.resolve();
            return defer.promise;
        }
        
        var srcConfigJson = sourceDir + '/config.json';
        var srcPeppyConf = sourceDir + '/' + PeppyConfBackupName;
        var srcSpectrumConf = sourceDir + '/' + SpectrumConfBackupName;
        
        if (!fs.existsSync(srcConfigJson) || !fs.existsSync(srcPeppyConf) || !fs.existsSync(srcSpectrumConf)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_FILES_MISSING'));
            defer.resolve();
            return defer.promise;
        }
        
        // Parse everything in the backup before touching live files, so a
        // corrupt backup is detected and rejected without any damage.
        try {
            JSON.parse(fs.readFileSync(srcConfigJson, 'utf8'));
        } catch (e) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_CONFIG_CORRUPT'));
            defer.resolve();
            return defer.promise;
        }
        try {
            ini.parse(fs.readFileSync(srcPeppyConf, 'utf8'));
        } catch (e) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_PEPPYCONF_CORRUPT'));
            defer.resolve();
            return defer.promise;
        }
        try {
            ini.parse(fs.readFileSync(srcSpectrumConf, 'utf8'));
        } catch (e) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_SPECTRUMCONF_CORRUPT'));
            defer.resolve();
            return defer.promise;
        }
        
        // All checks passed: perform the restore
        var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
        
        fs.copySync(srcConfigJson, configFile);
        fs.copySync(srcPeppyConf, MeterConfigFile);
        fs.copySync(srcSpectrumConf, SpectrumConfigFile);
        
        // Reload in-memory caches so subsequent saves do not stomp the restored values
        self.config.loadFile(configFile);
        meterConfig = ini.parse(fs.readFileSync(MeterConfigFile, 'utf-8'));
        base_folder_P = meterConfig.current['base.folder'] + '/';
        if (base_folder_P == '/') { base_folder_P = DATA_DIR + '/templates/'; }
        spectrum_config = ini.parse(fs.readFileSync(SpectrumConfigFile, 'utf-8'));
        base_folder_S = spectrum_config.current['base.folder'] + '/';
        if (base_folder_S == '/') { base_folder_S = DATA_DIR + '/templates_spectrum/'; }
        
        // Re-apply what the restored config.json decides: the display port
        // (in the launcher) and the share permissions. The audio path is the
        // tap on every source and needs nothing from the settings.
        var dispOut = parseInt(self.config.get('displayOutput'), 10);
        self.switch_DisplayPort(dispOut);

        var smbEnabled = self.config.get('smbShareAccess') === true;
        self.normalizeTemplatePermissions(smbEnabled);
        
        // Keep the doNotDeleteThemes .preserve flag file in sync with the
        // restored value so uninstall/install behaves as the restored
        // config.json expects.
        try {
            var doNotDelete = self.config.get('doNotDeleteThemes') === true;
            if (doNotDelete) {
                if (!fs.existsSync(DATA_DIR)) { fs.mkdirSync(DATA_DIR, { recursive: true }); }
                fs.writeFileSync(DATA_DIR + '/.preserve', '', 'utf8');
            } else {
                if (fs.existsSync(DATA_DIR + '/.preserve')) { fs.unlinkSync(DATA_DIR + '/.preserve'); }
            }
        } catch (e) {
            self.logger.warn(id + 'restoreSettingsBackup: preserve flag sync failed: ' + e.message);
        }
        
        // Update config version hash so remote clients pick up the new config
        self.updateConfigVersion();
        
        // Remove runFlag so peppymeter restarts on next trigger
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        
        self.logger.info(id + 'restoreSettingsBackup: restored backup "' + backupName + '"');
        self.commandRouter.pushToastMessage('success',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_RESTORED'));
        
        self.updateUIConfig();
        defer.resolve();
    } catch (e) {
        self.logger.error(id + 'restoreSettingsBackup: ' + e.message);
        self.commandRouter.pushToastMessage('error',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_RESTORE_FAILED'));
        defer.resolve();
    }
    
    return defer.promise;
};

// Delete a backup by name. Refuses to touch anything outside BackupsPath
// even if a traversal attempt somehow slips past the name regex.
Glass.prototype.deleteSettingsBackup = function (data) {
    var self = this;
    var defer = libQ.defer();
    
    try {
        var backupName = '';
        if (data && data.selectedBackupDelete) {
            backupName = (typeof data.selectedBackupDelete === 'object') ? data.selectedBackupDelete.value : String(data.selectedBackupDelete);
        }
        backupName = (backupName || '').trim();
        
        if (!backupName) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NOT_SELECTED'));
            defer.resolve();
            return defer.promise;
        }
        
        if (!BackupNameRegex.test(backupName) || backupName.indexOf('..') !== -1 || backupName.indexOf('/') !== -1 || backupName.indexOf('\\') !== -1) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NAME_INVALID'));
            defer.resolve();
            return defer.promise;
        }
        
        var targetDir = BackupsPath + '/' + backupName;
        if (!fs.existsSync(targetDir)) {
            self.commandRouter.pushToastMessage('error',
                self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
                self.commandRouter.getI18nString('GLASS.BACKUP_NOT_FOUND'));
            defer.resolve();
            return defer.promise;
        }
        
        // Belt-and-braces: resolve and confirm still within BackupsPath
        var resolved = path.resolve(targetDir);
        var resolvedRoot = path.resolve(BackupsPath);
        if (resolved.indexOf(resolvedRoot + '/') !== 0 && resolved !== resolvedRoot) {
            self.logger.error(id + 'deleteSettingsBackup: refusing to delete outside backups root: ' + resolved);
            defer.resolve();
            return defer.promise;
        }
        
        fs.removeSync(targetDir);
        
        self.logger.info(id + 'deleteSettingsBackup: deleted backup "' + backupName + '"');
        self.commandRouter.pushToastMessage('success',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_DELETED'));
        
        self.updateUIConfig();
        defer.resolve();
    } catch (e) {
        self.logger.error(id + 'deleteSettingsBackup: ' + e.message);
        self.commandRouter.pushToastMessage('error',
            self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.BACKUP_DELETE_FAILED'));
        defer.resolve();
    }
    
    return defer.promise;
};


Glass.prototype.setUIConfig = function(data) {
	var self = this;
	//Perform your installation tasks here
};

Glass.prototype.getConf = function(varName) {
	var self = this;
	//Perform your installation tasks here
};

Glass.prototype.setConf = function(varName, varValue) {
	var self = this;
	//Perform your installation tasks here
};
