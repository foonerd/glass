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
const os = require('os');
const { Manager, DEFAULT_PORT: MANAGER_DEFAULT_PORT } = require('./manager/server');
const { FaceFeed } = require('./manager/facefeed');
const { advanced } = require('./manager/statenow');
const screenprobe = require('./manager/screenprobe');
const playerfacts = require('./manager/playerfacts');
const screenowner = require('./manager/screenowner');
const views = require('./manager/views');
const relaunch = require('./manager/relaunch');
const picture = require('./manager/picture');
const graphics = require('./manager/graphics');
const automaticBackup = require('./manager/update').automaticBackup;
const facelook = require('./manager/facelook');
const component = require('./manager/component');
const { compact: compactQueue } = require('./manager/queue');
const legacyThemes = require('./manager/legacy');
const { safeFolderName, sections: configSections } = require('./manager/zip');

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
const logging = require('./manager/logging');
const spawn = require('child_process').spawn;
const performance = require('./manager/performance');
const ConfigDir = PluginPath + '/config';
const MeterConfigFile = ConfigDir + '/meter.txt';
const SpectrumConfigFile = ConfigDir + '/spectrum.txt';
const meterFolderStr = 'meter.folder';
const SpectrumFolderStr = 'spectrum.folder';
// What the tap measures to: the bank the spectrum theme on show asks for,
// written beside the rings for the tap to follow (tap::demand).
const SpectrumDemandFile = '/dev/shm/glasstap.demand';

// The plugin Glass replaces. Only one of the two may be enabled at a time.
const LEGACY_PLUGIN = 'peppy_screensaver';
const LEGACY_CONFIG = '/data/configuration/user_interface/peppy_screensaver/config.json';
const LEGACY_METER_CONFIG = '/data/plugins/user_interface/peppy_screensaver/screensaver/peppymeter/config.txt';
const LEGACY_DATA = '/data/INTERNAL/' + LEGACY_PLUGIN;

// The channel to the display: a local socket the plugin serves, one JSON
// object per line. A display that connects gets a greeting, then the player's
// state and the infinity flag last seen, then every change as it comes; it
// sends commands for the player back.
const channelPath = '/tmp/glass_channel';
// The frames daemon's socket for browser pages, which the manager proxies.
const faceSocketPath = '/tmp/glass_face.sock';
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
    // The clock when the state arrived, for a replay that moves the position on.
    this.stateAt = null;
    this.infinity = null;
    this.queue = null;
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
    if (self.state) { self.tell(conn, { kind: 'state', state: advanced(self.state, self.stateAt, Date.now()) }); }
    if (self.queue) { self.tell(conn, { kind: 'queue', items: self.queue }); }
    if (self.showing) { self.tell(conn, { kind: 'showing', theme: self.showing.theme, meter: self.showing.meter }); }
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
            } else if (message && message.kind === 'hello' && message.remote && typeof message.remote === 'object') {
                // A remote display says who it is.
                var r = message.remote;
                conn.remote = {
                    id: String(r.id || '').slice(0, 64),
                    name: String(r.name || '').slice(0, 64),
                    release: String(r.release || '').slice(0, 32),
                    screen: Array.isArray(r.screen) ? r.screen.slice(0, 2).map(function (n) { return parseInt(n, 10) || 0; }) : [0, 0],
                    // The remote's own settings page, when it serves one.
                    page: /^https?:\/\/[^\s"'<>]{1,150}$/.test(String(r.page || '')) ? String(r.page) : '',
                    address: String(conn.remoteAddress || '').replace(/^::ffff:/, ''),
                    since: new Date().toISOString()
                };
                self.logger.info(id + 'channel: remote ' + conn.remote.name + ' (' + conn.remote.address + ', ' + conn.remote.release + ')');
            } else if (message && message.kind === 'showing' && !conn.remote) {
                // The player's own display says which meter it shows; the
                // remotes that follow the player hear of it.
                var rate = parseInt(message.rate, 10);
                self.showing = { theme: String(message.theme || '').slice(0, 128), meter: String(message.meter || '').slice(0, 128), rate: (rate >= 1 && rate <= 240) ? rate : null };
                self.clients.slice().forEach(function (c) {
                    if (c.remote) { self.tell(c, { kind: 'showing', theme: self.showing.theme, meter: self.showing.meter }); }
                });
                if (self.onPush) { self.onPush({ kind: 'showing', theme: self.showing.theme, meter: self.showing.meter }); }
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

// The same channel over TCP, for remote displays on the network.
Channel.prototype.listenTcp = function (port) {
    var self = this;
    self.tcpServer = net.createServer(function (conn) {
        conn.setNoDelay(true);
        self.attach(conn);
    });
    self.tcpError = null;
    self.tcpServer.on('error', function (err) {
        self.tcpError = 'tcp ' + port + ': ' + (err && err.message ? err.message : err);
        self.logger.error(id + 'channel: ' + self.tcpError);
        self.tcpServer = null;
    });
    self.tcpServer.listen(port, function () {
        self.logger.info(id + 'channel: serving tcp ' + port);
    });
};

// Close the TCP listener and the remotes on it. Resolves once the port is free.
Channel.prototype.unlistenTcp = function () {
    var self = this;
    self.clients.slice().forEach(function (c) {
        if (c.remote || (c.remoteAddress && c.remoteAddress !== '')) {
            try { c.destroy(); } catch (e) {}
        }
    });
    var server = self.tcpServer;
    self.tcpServer = null;
    self.tcpError = null;
    if (!server) { return Promise.resolve(); }
    return new Promise(function (resolve) {
        var done = false;
        var finish = function () { if (!done) { done = true; resolve(); } };
        try { server.close(finish); } catch (e) { finish(); }
        setTimeout(finish, 2000);
    });
};

// The remote displays connected, as they introduced themselves.
Channel.prototype.remotes = function () {
    return this.clients.filter(function (c) { return c.remote; }).map(function (c) { return c.remote; });
};

Channel.prototype.tell = function (conn, message) {
    try { conn.write(JSON.stringify(message) + '\n'); } catch (e) {}
};

// Every connected display hears this, and whoever mirrors the channel.
Channel.prototype.push = function (message) {
    var self = this;
    self.clients.slice().forEach(function (conn) { self.tell(conn, message); });
    if (self.onPush) { self.onPush(message); }
};

Channel.prototype.close = function () {
    var self = this;
    self.clients.slice().forEach(function (conn) { try { conn.destroy(); } catch (e) {} });
    self.clients = [];
    if (self.server) {
        try { self.server.close(); } catch (e) {}
        self.server = null;
    }
    if (self.tcpServer) {
        try { self.tcpServer.close(); } catch (e) {}
        self.tcpServer = null;
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

// How long after the plugin's stop the registry is asked whether the stop
// was a turning off: the plugin manager disables it right after the stop.
var STOP_GUARD_MS = 5000;

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
// Fonts the listener uploads live under DATA_DIR too, and reach the remotes.
const CustomFontsPath = DATA_DIR + '/fonts';
const fontFiles = require('./manager/fonts');
const cardash = require('./manager/cardash');
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
const ALSA_FILE = '/etc/asound.conf';

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
    // Every line the plugin writes passes a level and, at the fine levels,
    // a target gate set on the manager's Status tab.
    self.logger = logging.makeLogger(self.context.logger, id, function () { return self.logSettings(); });
    self.configManager = self.context.configManager;
}

// The logging settings: the level and, at verbose and trace, the targets.
Glass.prototype.logSettings = function () {
    var self = this;
    var level = null;
    var targets = [];
    try {
        level = self.config ? self.config.get('logLevel') : null;
        var raw = self.config ? self.config.get('logTargets') : '';
        targets = String(raw || '').split(',').map(function (t) { return t.trim(); }).filter(Boolean);
    } catch (e) { /* before the configuration is loaded */ }
    return logging.normalize({ level: level, targets: targets });
};

Glass.prototype.setLogSettings = function (data) {
    var self = this;
    var wanted = logging.normalize(data || {});
    var before = self.logSettings();
    self.config.set('logLevel', wanted.level);
    self.config.set('logTargets', wanted.targets.join(','));
    // On disk at once: the settings are saved a second after a change by
    // themselves, and a level set just before a restart must outlive it.
    try { self.config.save(); } catch (e) { /* saved by itself a second on */ }
    var changed = before.level !== wanted.level || before.targets.join(',') !== wanted.targets.join(',');
    if (changed) { self.context.logger.info(id + 'logging: level ' + wanted.level + (wanted.targets.length ? ', targets ' + wanted.targets.join(', ') : '')); }
    return Object.assign({ changed: changed }, self.logSettings());
};

// Start the display again when it is up, so it reads its settings and its
// log level afresh; says whether there was one to start again.
Glass.prototype.relaunchDisplay = function () {
    var self = this;
    var running = !!(self.meterChild && self.meterChild.exitCode === null);
    if (running && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    return running;
};

// The player's system log through Volumio's own submitter, the call the
// player's dev page makes: the backend runs the submitter and broadcasts
// what it said, the log server's answer with the link, to its interfaces,
// of which this plugin is one. Resolves with those words; a call made
// while another waits ends the earlier one with nothing.
Glass.prototype.sendSystemLog = function (description) {
    var self = this;
    return new Promise(function (resolve, reject) {
        if (self.logReply) { self.logReply(''); }
        self.logReply = resolve;
        try {
            self.commandRouter.executeOnPlugin('system_controller', 'system', 'sendBugReport', { text: String(description || '') });
        } catch (e) {
            self.logReply = null;
            reject(e);
        }
    });
};

// What Volumio's backend broadcasts to its interfaces, as a name and a
// value or as one { msg, value }, by its version. Only the submitter's
// answer is of interest here.
Glass.prototype.broadcastMessage = function (emit, payload) {
    var named = emit && typeof emit === 'object';
    var name = named ? emit.msg : emit;
    var value = named ? emit.value : payload;
    if (name === 'pushSendBugReport' && this.logReply) {
        var answer = this.logReply;
        this.logReply = null;
        answer(typeof value === 'string' ? value : JSON.stringify(value || ''));
    }
};

// The last lines Glass wrote to the player's journal, newest last.
Glass.prototype.recentLog = function (count) {
    var wanted = Math.min(Math.max(parseInt(count, 10) || 300, 20), 3000);
    var out = '';
    try {
        out = require('child_process').execFileSync('/bin/journalctl', ['-u', 'volumio', '-n', String(wanted * 8), '--no-pager', '-o', 'short-iso'], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 15000 });
    } catch (e) {
        return { error: 'journal', message: String(e && e.message ? e.message : e), lines: [] };
    }
    var lines = out.split('\n').filter(function (l) { return /glass/i.test(l); });
    return { lines: lines.slice(-wanted), total: lines.length };
};

// The status sheet's rows only the plugin knows: the player and its
// addresses, the screen, the audio path, and the housekeeping. A support
// question is answered from a copy of the sheet, so every row here is
// one a triage needs and nothing about the wider network.
Glass.prototype.sheetInfo = function () {
    var self = this;
    var state = self.lastState || {};
    var release = {};
    try {
        fs.readFileSync('/etc/os-release', 'utf8').split('\n').forEach(function (line) {
            var m = /^([A-Z_]+)="?([^"]*)"?$/.exec(line.trim());
            if (m) { release[m[1]] = m[2]; }
        });
    } catch (e) {}
    var addresses = [];
    try {
        var interfaces = os.networkInterfaces();
        Object.keys(interfaces).forEach(function (name) {
            (interfaces[name] || []).forEach(function (a) {
                if (a.family === 'IPv4' && !a.internal) { addresses.push({ name: name, address: a.address }); }
            });
        });
    } catch (e) {}
    var build = null;
    try { build = JSON.parse(fs.readFileSync(PluginPath + '/build.json', 'utf8')); } catch (e) {}
    var tapInChain = false;
    try { tapInChain = fs.readFileSync('/etc/asound.conf', 'utf8').indexOf('Glass section') !== -1; } catch (e) {}
    var output = '';
    try { output = String(self.commandRouter.sharedVars.get('alsa.outputdevicename') || self.commandRouter.sharedVars.get('alsa.outputdevice') || ''); } catch (e) {}
    var problems = [];
    try {
        var log = self.recentLog(300);
        problems = (log.lines || []).filter(function (l) { return /\b(warn|warning|error)\b/i.test(l); }).slice(-3);
    } catch (e) {}
    var newest = null;
    try {
        var backups = self.backupList() || [];
        backups.forEach(function (b) {
            var at = b.date || b.at || b.created || '';
            if (!newest || String(at) > String(newest.at)) { newest = { name: b.name, at: at }; }
        });
    } catch (e) {}
    var mouse = true;
    try { mouse = String((meterConfig && meterConfig.sdl && meterConfig.sdl.env && meterConfig.sdl.env['mouse.enabled']) || 'True').toLowerCase() === 'true'; } catch (e) {}
    return {
        player: {
            volumio: release.VOLUMIO_VERSION || '',
            hardware: release.VOLUMIO_HARDWARE || '',
            board: self.boardInfo().model,
            backendUptimeS: Math.round(process.uptime()),
            build: build,
            memory: self.playerMemory(),
            plugins: self.otherPlugins()
        },
        addresses: addresses,
        screen: { size: self.screenSize(), mouse: mouse, since: self.displayStartedAt || null },
        audio: {
            tapInChain: tapInChain,
            output: output,
            service: state.service || '',
            trackType: state.trackType || '',
            samplerate: state.samplerate || '',
            bitdepth: state.bitdepth || ''
        },
        housekeeping: { newestBackup: newest, problems: problems }
    };
};

// The player's memory as the kernel says it now; null where it does not.
Glass.prototype.playerMemory = function () {
    try { return playerfacts.memoryOf(fs.readFileSync('/proc/meminfo', 'utf8')); } catch (e) { return null; }
};

// The other plugins installed on the player, with whether each is on and
// whether it is in the audio path: what someone reading a report needs to
// know beside Glass's own state.
Glass.prototype.otherPlugins = function () {
    var installed = [];
    var root = '/data/plugins';
    try {
        fs.readdirSync(root).forEach(function (category) {
            var names = [];
            try { names = fs.readdirSync(root + '/' + category); } catch (e) { return; }
            names.forEach(function (name) {
                try { installed.push({ category: category, name: name, package: fs.readFileSync(root + '/' + category + '/' + name + '/package.json', 'utf8') }); } catch (e) { /* not a plugin's folder */ }
            });
        });
    } catch (e) { return []; }
    var registry = {};
    try { registry = JSON.parse(fs.readFileSync('/data/configuration/plugins.json', 'utf8')); } catch (e) {}
    return playerfacts.pluginsOf(installed, registry);
};

// The screen's size as the X server has it, asked at most once a minute;
// empty without a screen or an X server.
Glass.prototype.screenSize = function () {
    var self = this;
    var now = Date.now();
    if (self.screenSizeAt && now - self.screenSizeAt < 60000) { return self.screenSizeKnown || ''; }
    self.screenSizeAt = now;
    self.screenSizeKnown = '';
    var display = ':' + String(self.config.get('displayOutput') || '0').replace(/^:/, '');
    var auth = '';
    try {
        var auths = fs.readdirSync('/tmp').filter(function (n) { return n.indexOf('serverauth.') === 0; }).map(function (n) { return '/tmp/' + n; });
        auths.sort(function (a, b) { return fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs; });
        auth = auths[0] || '';
    } catch (e) {}
    try {
        var out = require('child_process').execFileSync('xrandr', ['--current'], { encoding: 'utf8', timeout: 3000, env: Object.assign({}, process.env, { DISPLAY: display }, auth ? { XAUTHORITY: auth } : {}) });
        var m = /current\s+(\d+)\s*x\s*(\d+)/.exec(out);
        if (m) { self.screenSizeKnown = m[1] + 'x' + m[2]; }
    } catch (e) {}
    return self.screenSizeKnown;
};

// The board this player is, for the performance profiles.
Glass.prototype.boardInfo = function () {
    var model = '';
    try { model = fs.readFileSync('/proc/device-tree/model', 'utf8').replace(/\0/g, '').trim(); } catch (e) {}
    var cores = 1;
    try { cores = require('os').cpus().length || 1; } catch (e) {}
    var arch = process.arch === 'x64' ? 'x86_64' : (require('os').arch() || '');
    return { model: model, cores: cores, arch: arch, class: performance.boardClass(model, cores, arch) };
};

// The performance profile: the values as they are, which profile they
// are, the one chosen, and the one the board would be given.
Glass.prototype.performanceInfo = function () {
    var self = this;
    self.loadConfigs();
    var current = (meterConfig && meterConfig.current) || {};
    var values = {
        frameRate: parseInt(current['frame.rate'], 10) || 30,
        rotationQuality: String(current['rotation.quality'] || 'medium'),
        rotationFps: parseInt(current['rotation.fps'], 10) || 8,
        transitions: String(current['transition.type'] || 'fade') !== 'none'
    };
    var governor = String(current['frame.rate.governor'] || 'True').toLowerCase() !== 'false';
    var board = self.boardInfo();
    var height = parseInt(current['screen.height'], 10) || 720;
    var auto = performance.autoProfile(board.class, height);
    // Until a profile is chosen, the values say which one they are.
    var chosen = String(self.config.get('perfProfile') || '');
    if (performance.NAMES.indexOf(chosen) === -1) { chosen = performance.nameOf(values); }
    return { profile: chosen, auto: auto, values: values, valuesProfile: performance.nameOf(values), board: board, profiles: performance.PROFILES, governor: governor, showing: self.channel && self.channel.showing ? self.channel.showing : null };
};

// Whether the display may lower its frame rate when it cannot keep up.
Glass.prototype.setGovernor = function (on) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var wanted = on ? 'True' : 'False';
    if (String(meterConfig.current['frame.rate.governor'] || 'True') === wanted) { return { changed: false }; }
    meterConfig.current['frame.rate.governor'] = wanted;
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    self.logger.info(id + 'performance: governor ' + (on ? 'on' : 'off'));
    return { changed: true };
};

// The fonts the text is set in, one per style. A style's font.<style> value
// in the meter configuration is `builtin` (the plugin's multi-script
// PeppyFont face of that style, DSEG7 for the clock), the name of a file
// under font.path (the player's own fonts, Volumio's Lato), or the path of a
// font the listener uploaded under CustomFontsPath.
var FONT_STYLES = ['light', 'regular', 'bold', 'italic', 'digi'];
var BUILTIN_FACES = { light: 'PeppyFont-Light.ttf', regular: 'PeppyFont-Regular.ttf', bold: 'PeppyFont-Bold.ttf', italic: 'PeppyFont-Italic.ttf', digi: 'DSEG7Classic-Italic.ttf' };

function customFontPath(name) {
    if (typeof name !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,127}\.(ttf|otf)$/i.test(name)) { return null; }
    var file = CustomFontsPath + '/' + name;
    try { return fs.statSync(file).isFile() ? file : null; } catch (e) { return null; }
}

// What a style's configured value means to the panel: `builtin`, a player
// font by name, an uploaded font by name, or the value as it stands.
function fontChoice(value) {
    var v = String(value || '').trim();
    if (!v || v.toLowerCase() === 'builtin') { return { kind: 'builtin', value: 'builtin' }; }
    if (v.indexOf('/') === 0) {
        var name = path.basename(v);
        if (path.dirname(v) === CustomFontsPath && customFontPath(name)) { return { kind: 'custom', value: name }; }
    }
    var plain = v.replace(/^\/+/, '');
    var dir = fontPathDir();
    if (plain.indexOf('/') === -1 && dir && fs.existsSync(dir + '/' + plain)) { return { kind: 'player', value: plain }; }
    return { kind: 'other', value: v };
}

Glass.prototype.fontsSettings = function () {
    var self = this;
    self.loadConfigs();
    var current = (meterConfig && meterConfig.current) || {};
    var styles = {};
    FONT_STYLES.forEach(function (style) { styles[style] = fontChoice(current['font.' + style]); });
    var builtIn = {};
    FONT_STYLES.forEach(function (style) { builtIn[style] = fs.existsSync(PluginPath + '/fonts/' + BUILTIN_FACES[style]); });
    var dir = fontPathDir();
    return {
        styles: styles,
        builtIn: builtIn,
        player: { path: dir || '', fonts: dir ? filesOf(dir, isFontFile).map(function (f) { return f.name; }) : [] },
        custom: filesOf(CustomFontsPath, isFontFile).map(function (f) { return { name: f.name, bytes: f.bytes }; })
    };
};

// Set styles: `{ styles: { light: 'builtin' | '<player font>' | { custom: '<name>' } } }`.
// A style not given keeps its value. The display starts again with a change.
Glass.prototype.setFontsSettings = function (data) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var wanted = (data && data.styles) || {};
    var writes = {};
    var dir = fontPathDir();
    for (var i = 0; i < FONT_STYLES.length; i++) {
        var style = FONT_STYLES[i];
        if (wanted[style] === undefined) { continue; }
        var choice = wanted[style];
        var value;
        if (choice && typeof choice === 'object' && typeof choice.custom === 'string') {
            value = customFontPath(choice.custom);
            if (!value) { return { error: 'bad-font', style: style }; }
        } else {
            var v = String(choice || 'builtin').trim();
            if (v.toLowerCase() === 'builtin') { value = 'builtin'; }
            else if (/^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,127}$/.test(v) && dir && fs.existsSync(dir + '/' + v)) { value = v; }
            else { return { error: 'bad-font', style: style }; }
        }
        writes[style] = value;
    }
    var changed = false;
    Object.keys(writes).forEach(function (style) {
        if (String(meterConfig.current['font.' + style] || '') !== writes[style]) { meterConfig.current['font.' + style] = writes[style]; changed = true; }
    });
    if (meterConfig.current['use.system.fonts'] !== undefined) { delete meterConfig.current['use.system.fonts']; changed = true; }
    if (!changed) { return { changed: false }; }
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    self.logger.info(id + 'fonts: ' + Object.keys(writes).map(function (s) { return s + '=' + writes[s]; }).join(' '));
    return { changed: true };
};

// An older configuration's one switch, use.system.fonts, folded into the
// per-style values once: False (the default, the built-in set) names the
// built-in face for every style; True keeps the names under font.path.
Glass.prototype.migrateFontsConfig = function () {
    var self = this;
    if (!meterConfig || !meterConfig.current || meterConfig.current['use.system.fonts'] === undefined) { return; }
    var system = String(meterConfig.current['use.system.fonts']).trim().toLowerCase() === 'true';
    if (!system) {
        ['light', 'regular', 'bold', 'italic'].forEach(function (style) { meterConfig.current['font.' + style] = 'builtin'; });
    }
    delete meterConfig.current['use.system.fonts'];
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    self.logger.info(id + 'fonts: use.system.fonts folded into the styles (' + (system ? 'the player fonts' : 'built-in') + ')');
};

// A font file received into `file` is kept under CustomFontsPath by a plain
// name, when its bytes are a TrueType or OpenType face. Re-uploading a name
// replaces it; the display starts again if a style is set in it.
Glass.prototype.addCustomFont = function (file, rawName) {
    var self = this;
    var head = Buffer.alloc(4);
    var fd;
    try {
        fd = fs.openSync(file, 'r');
        fs.readSync(fd, head, 0, 4, 0);
    } catch (e) { return { error: 'not-a-font' }; } finally { if (fd !== undefined) { try { fs.closeSync(fd); } catch (e) {} } }
    var kind = fontFiles.fontKind(head);
    if (!kind) { return { error: 'not-a-font' }; }
    var name = fontFiles.safeFontName(rawName, kind);
    if (!name) { return { error: 'bad-name' }; }
    fs.ensureDirSync(CustomFontsPath);
    fs.moveSync(file, CustomFontsPath + '/' + name, { overwrite: true });
    self.loadConfigs();
    var inUse = self.stylesIn(name).length > 0;
    if (inUse) {
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    }
    self.pushRemoteConfig();
    self.logger.info(id + 'fonts: uploaded ' + name + (inUse ? ' (in use)' : ''));
    return { name: name };
};

// The styles set in the uploaded font of that name.
Glass.prototype.stylesIn = function (name) {
    var current = (meterConfig && meterConfig.current) || {};
    return FONT_STYLES.filter(function (style) {
        var v = String(current['font.' + style] || '').trim();
        return v.indexOf('/') === 0 && path.dirname(v) === CustomFontsPath && path.basename(v) === name;
    });
};

// Remove an uploaded font; a style set in it goes back to the built-in face.
Glass.prototype.removeCustomFont = function (name) {
    var self = this;
    var file = customFontPath(name);
    if (!file) { return { error: 'not-found' }; }
    self.loadConfigs();
    var freed = self.stylesIn(name);
    fs.removeSync(file);
    if (freed.length && meterConfig) {
        freed.forEach(function (style) { meterConfig.current['font.' + style] = 'builtin'; });
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    }
    self.pushRemoteConfig();
    self.logger.info(id + 'fonts: removed ' + name + (freed.length ? ' (' + freed.join(', ') + ' back to built-in)' : ''));
    return { removed: name, freed: freed };
};

// Apply a profile: its values into the meter configuration and the
// settings page's mirrors; the display starts again with them.
Glass.prototype.applyPerformanceProfile = function (name) {
    var self = this;
    var chosen = String(name || 'auto');
    if (performance.NAMES.indexOf(chosen) === -1) { return { error: 'bad-profile' }; }
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    self.config.set('perfProfile', chosen);
    if (chosen === 'custom') { return Object.assign({ changed: false }, self.performanceInfo()); }
    var info = self.performanceInfo();
    var target = chosen === 'auto' ? info.auto : chosen;
    var p = performance.PROFILES[target];
    var changed = false;
    var set = function (key, value) {
        if (String(meterConfig.current[key]) !== String(value)) { meterConfig.current[key] = value; changed = true; }
    };
    set('frame.rate', p.frameRate);
    set('rotation.quality', p.rotationQuality);
    set('rotation.fps', p.rotationFps);
    set('transition.type', p.transitions ? 'fade' : 'none');
    set('start.animation', p.transitions ? 'True' : 'False');
    self.config.set('frameRate', p.frameRate);
    self.config.set('rotationQuality', p.rotationQuality);
    self.config.set('rotationFPS', p.rotationFps);
    self.config.set('transitionType', p.transitions ? 'fade' : 'none');
    self.config.set('animation', p.transitions);
    if (changed) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        uiNeedsUpdate = true;
        self.updateUIConfig();
        self.logger.info(id + 'performance: profile ' + chosen + (chosen === 'auto' ? ' (' + target + ')' : '') + ': ' + p.frameRate + ' fps, rotation ' + p.rotationQuality);
    }
    return Object.assign({ changed: changed }, self.performanceInfo());
};

// A setting changed by hand on the settings page leaves the profile.
Glass.prototype.profileTouched = function () {
    var self = this;
    try {
        if (String(self.config.get('perfProfile') || 'auto') !== 'custom') { self.config.set('perfProfile', 'custom'); }
    } catch (e) {}
};

Glass.prototype.onVolumioStart = function () {
    var self = this;
    var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
    self.config = new (require('v-conf'))();
    self.config.loadFile(configFile);
    var defaults = {
        themeTagRules: ['string', ''],
        legacyImported: ['boolean', false],
        displayOutput: ['string', '0'],
        headless: ['boolean', false],
        managerPort: ['number', MANAGER_DEFAULT_PORT],
        managerHost: ['string', ''],
        remotesEnabled: ['boolean', false],
        remoteFramesPort: ['number', 5580],
        remoteChannelPort: ['number', 5581],
        remoteBeaconPort: ['number', 5579]
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
    // The player's architecture does not change under a running plugin.
    if (this.archKnown) { return this.archKnown; }
    try {
        return this.archKnown = execSync('cat /etc/os-release | grep ^VOLUMIO_ARCH | tr -d \'VOLUMIO_ARCH="\'').toString().trim();
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
    this.writeSpectrumDemand();
};

// The bank count a theme of the previous engine is measured at: its bar
// count rounded up to the next of 32, 64, 128 and 256.
function binsForLegacy(size) {
    var n = parseInt(size, 10) || 20;
    var steps = [32, 64, 128, 256];
    for (var i = 0; i < steps.length; i++) { if (steps[i] >= n) { return steps[i]; } }
    return 256;
}

// What the spectrum theme on show asks of the tap: over every section of
// its spectrum.txt, the most bands, two channels when any asks or none
// says, the first scale named, the longest window named; a theme that
// names nothing is measured at its bar count rounded up, on the log scale.
Glass.prototype.spectrumDemand = function () {
    var current = (spectrum_config && spectrum_config.current) || {};
    var folder = String(current[SpectrumFolderStr] || '').trim();
    var sections = {};
    try {
        if (folder) { sections = ini.parse(fs.readFileSync(base_folder_S + folder + '/spectrum.txt', 'utf-8')); }
    } catch (e) { sections = {}; }
    var bins = 0, channels = 0, scale = '', window = 0, named = false;
    Object.keys(sections).forEach(function (name) {
        var section = sections[name];
        if (!section || typeof section !== 'object' || name === 'current') { return; }
        var b = parseInt(section.bins, 10), c = parseInt(section.channels, 10), w = parseInt(section.window, 10);
        var sc = String(section.scale || '').trim().toLowerCase();
        if (b > 0) { bins = Math.max(bins, b); named = true; }
        if (c === 1 || c === 2) { channels = Math.max(channels, c); named = true; }
        var side = String(section.channel || '').trim().toLowerCase();
        if (side === 'left' || side === 'right' || side === 'l' || side === 'r') { channels = 2; named = true; }
        if (!scale && (sc === 'log' || sc === 'mel' || sc === 'linear')) { scale = sc; named = true; }
        if (w > 0) { window = Math.max(window, w); named = true; }
    });
    if (!named) { bins = binsForLegacy(current.size); }
    return {
        bins: Math.min(256, Math.max(1, bins || 256)),
        channels: channels || 2,
        scale: scale || 'log',
        window: window || 0
    };
};

// Write the demand beside the rings when it changed; the tap picks it up
// within a second and measures anew.
Glass.prototype.writeSpectrumDemand = function () {
    var self = this;
    var text;
    try { text = JSON.stringify(self.spectrumDemand()); } catch (e) { return; }
    if (self.lastSpectrumDemand === text) { return; }
    try {
        fs.writeFileSync(SpectrumDemandFile + '.part', text);
        fs.renameSync(SpectrumDemandFile + '.part', SpectrumDemandFile);
        self.lastSpectrumDemand = text;
        if (self.logger) { self.logger.info(id + 'spectrum demand ' + text); }
    } catch (e) {
        if (self.logger) { self.logger.warn(id + 'spectrum demand not written: ' + (e && e.message ? e.message : e)); }
    }
};

// The environment the display is launched with: its home, the X display
// the settings name, and the marker a real touch writes.
Glass.prototype.launchEnv = function () {
    var self = this;
    var display = String(self.config.get('displayOutput') || '0').replace(/[^0-9]/g, '') || '0';
    var log = self.logSettings();
    var env = Object.assign({}, process.env, {
        DISPLAY: ':' + display,
        GLASS_HOME: PluginPath,
        GLASS_DISMISS_FILE: dismissFile,
        GLASS_CHANNEL: channelPath,
        GLASS_LOG: log.level,
        GLASS_LOG_TARGETS: log.targets.join(',')
    });
    // The screen by fact: X while a kiosk runs one, the screen itself while
    // no kiosk uses it; otherwise no word, and the display opens no window.
    var draws = null;
    var ours = false;
    try { var fact = self.screenFact(); draws = screenprobe.wouldDraw(fact); ours = screenprobe.screenOwn(fact); } catch (e) {}
    if (draws) { env.SDL_VIDEODRIVER = draws; } else { delete env.SDL_VIDEODRIVER; }
    // On the screen itself SDL is told which card has the screen: its own search gives up or not by the
    // order the system lists the cards in. Said here too, for a face built before the display said it itself.
    delete env.SDL_KMSDRM_DEVICE_INDEX;
    if (draws === 'kmsdrm') {
        try { var card = screenprobe.kmsCard(self.screenProbe(false)); if (card !== null) { env.SDL_KMSDRM_DEVICE_INDEX = String(card); } } catch (e) { /* the display chooses */ }
    }
    // An X server brought up for the face is the display's own screen: it stays on it and turns the picture itself.
    if (ours) { env.GLASS_SCREEN_OURS = '1'; } else { delete env.GLASS_SCREEN_OURS; }
    // When glass-evo owns the screen and its component is here, the face
    // runs in the display's place: the same launcher, another binary.
    delete env.GLASS_BIN;
    try {
        var owner = self.screenOwnerState();
        if (owner.owner === 'glass-evo' && owner.evo.available) { env.GLASS_BIN = owner.evo.binary; }
    } catch (e) {}
    // Where face themes are kept, the user's before the ones glass-evo
    // ships, for the face to read the one chosen.
    env.GLASS_FACES = FACES_DIR + ':' + EVO_LOOKS_DIR;
    return env;
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
    // The display's answer to a calibration is not a command for the player.
    if (name === 'calibration') { self.takeCalibration(value); return; }
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
        try { self.pushPersist('', 0, 0); } catch (e) {}

    // The channel the display reads the player's state from. A display that
    // connects gets fresh values asked of the player.
    self.channel = new Channel(self.logger, function () {
        socket.emit('getState', '');
        socket.emit('getInfinityPlayback', '');
        socket.emit('getQueue', '');
        // A remote that connects hears the configuration as it stands, so a
        // change made while it was away is not missed.
        self.pushRemoteConfig();
    }, function (message) {
        self.runCommand(message);
    });
    self.channel.listen(channelPath);

    // The feed behind the manager's Face tab: the frames from the daemon's
    // pages socket, and every line the displays hear, for browser pages.
    self.face = new FaceFeed({ socketPath: faceSocketPath, logger: self.logger, current: function () {
        return self.channel ? { state: advanced(self.channel.state, self.channel.stateAt, Date.now()), infinity: self.channel.infinity, showing: self.channel.showing, queue: self.channel.queue, persist: self.channel.persist } : {};
    } });
    self.channel.onPush = function (message) { self.face.push(message); };
    // The pointer, resolved from the screen's choice and what the player has now.
    try { self.refreshPointerShown(); } catch (e) { self.logger.warn(id + 'screen: pointer not resolved: ' + (e && e.message ? e.message : e)); }
    // The screen by fact, watched from now on; a driver key from the old choice goes back to Auto.
    self.pluginStartedAt = Date.now();
    try { self.migrateScreenDriver(); } catch (e) { self.logger.warn(id + 'screen: driver key not migrated: ' + (e && e.message ? e.message : e)); }
    try { self.watchScreen(); } catch (e) { self.logger.warn(id + 'screen: not watched: ' + (e && e.message ? e.message : e)); }
    // The graphics are looked at once the player has settled, so the first page that asks is told.
    setTimeout(function () { try { self.checkGraphics().catch(function () { self.graphicsChecking = null; }); } catch (e) { /* at the first page's look */ } }, 20000);
    // The board's word on frost is kept for the face, on a player that has one.
    try { if (self.screenOwnerState().here) { self.noteFrostSuits(); } } catch (e) { self.logger.warn(id + 'face: the board not noted: ' + (e && e.message ? e.message : e)); }

    self.loadConfigs();
    if (!meterConfig) {
        self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'), self.commandRouter.getI18nString('GLASS.NO_PEPPYCONFIG'));
        return libQ.reject(new Error('meter configuration missing'));
    }
    try { self.importLegacySettings(); } catch (e) {
        self.logger.warn(id + 'settings import: ' + (e && e.message ? e.message : e));
    }
    try { self.migrateFontsConfig(); } catch (e) {
        self.logger.warn(id + 'fonts: ' + (e && e.message ? e.message : e));
    }

    // The audio path: the tap heads the ALSA contribution and meters every
    // source. What earlier releases added beside it is taken back once.
    self.retireSideOutputs(false)
        .then(self.writeAsoundConfigModular.bind(self))
        .then(self.updateALSAConfigFile.bind(self))
        .then(function () { return self.nudgeSoloist(); })
        .then(function () { self.watchAlsaFile(); })
        .fail(function (e) {
            self.logger.error(id + 'audio path: ' + (e && e.message ? e.message : e));
        });

    // The frames daemon: for the browser pages always, and with remote
    // displays served, the port, the channel over TCP and the beacon too.
    try {
        if (self.remotePorts().enabled) { self.startRemotes(); } else { self.startServe(); }
    } catch (e) {
        self.logger.error(id + 'remotes: ' + (e && e.message ? e.message : e));
    }

    // Themes still read from the old plugin's folder come into Glass's own
    // first; then the manager, the web application on its own port.
    self.adoptLegacyThemes().then(function () {
        if (self.config.get('smbShareAccess') === true) { self.normalizeTemplatePermissions(true); }
        try { self.armCarDash(); } catch (e) { self.logger.warn(id + 'car dash: ' + (e && e.message ? e.message : e)); }
        return self.startManager();
    }).catch(function () {
        self.commandRouter.pushToastMessage('error', self.commandRouter.getI18nString('GLASS.PLUGIN_NAME'),
            self.commandRouter.getI18nString('GLASS.MANAGER_PORT_IN_USE') + ' ' + (parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT));
    });

    // The player state drives the display: it opens after the timeout while
    // music plays, stays through a pause for the persist time, and leaves
    // when the run flag goes.
    socket.emit('getState', '');
    socket.emit('getInfinityPlayback', '');
    socket.emit('getQueue', '');
    var lastService = '';
    var lastUri = '';

    socket.on('pushInfinityPlayback', function (data) {
        var on = !!(data && data.enabled);
        if (self.channel) {
            self.channel.infinity = on;
            self.channel.push({ kind: 'infinity', on: on });
        }
    });

    // The queue, as the player answers getQueue and pushes it on every
    // change: displays draw the next track and the queue's length from it,
    // the Face among them, which cannot ask the player itself.
    socket.on('pushQueue', function (queue) {
        var items = compactQueue(queue);
        if (self.channel) {
            self.channel.queue = items;
            self.channel.push({ kind: 'queue', items: items });
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
            self.channel.stateAt = Date.now();
            // The cover's addresses of late, for the route that fetches only what the player reported.
            self.channel.arts = picture.reported(self.channel.arts, state && state.albumart);
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
        try { self.pushPersist('', 0, 0); } catch (e) {}
            try { self.applyThemeTag(state); } catch (eTag) {
                self.logger.warn(id + 'theme tag: ' + (eTag && eTag.message ? eTag.message : eTag));
            }

            if (!self.Timeout) {
                lastStateIsPlaying = true;
                var ScreenTimeout = (parseInt(self.config.get('timeout'), 10)) * 1000;

                if (ScreenTimeout > 0) {
                    var startDisplayOnce = function () { self.startDisplayOnce(); };
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
                    self.pushPersist(persistDisplay, persistDuration, Date.now());
                    self.persistTimer = setTimeout(function () {
                        self.persistTimer = null;
                        try { if (fs.existsSync(persistFile)) fs.removeSync(persistFile); } catch (e) {}
        try { self.pushPersist('', 0, 0); } catch (e) {}
                        if (self.screenOurs()) {
                            self.logger.info(id + 'persist timer expired; the screen is ours, the display stays');
                        } else if (fs.existsSync(runFlag)) {
                            fs.removeSync(runFlag);
                            self.logger.info(id + 'persist timer expired, the display leaves');
                        }
                        lastStateIsPlaying = false;
                    }, persistDuration * 1000);
                } else {
                    if (!self.screenOurs() && fs.existsSync(runFlag)) {
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
                    self.pushPersist('', 0, 0);
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
    // glass-evo's screen goes back to the kiosk when the plugin is turned
    // off or removed. An update stops the plugin too, and leaves it enabled:
    // a moment after the stop the player's registry says which it was.
    setTimeout(function () {
        try { self.guardScreen(true); } catch (e) { self.logger.warn(id + 'screen owner: not guarded after the stop: ' + (e && e.message ? e.message : e)); }
    }, STOP_GUARD_MS);
    if (self.carDashTimer) { clearTimeout(self.carDashTimer); self.carDashTimer = null; }
    if (self.screenWatcher) { clearTimeout(self.screenWatcher); self.screenWatcher = null; }

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
        try { self.pushPersist('', 0, 0); } catch (e) {}

        self.commandRouter.removePluginRestEndpoint({ endpoint: 'glass_artistfanart' });
        socket.off('pushState');
        socket.off('pushInfinityPlayback');
        if (self.channel) {
            self.channel.close();
            self.channel = null;
        }
        if (self.face) {
            self.face.stop();
            self.face = null;
        }
        self.unwatchAlsaFile();
        self.stopRemotes();
        self.stopManager();
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
    self.nudgeSoloist();
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
// ---- Interactive controls: whether a theme's buttons and indicators act --

// `theme`, `on` or `off` from any spelling; `theme` for anything else.
Glass.prototype.interactiveModeOf = function (value) {
    var v = String(value === undefined || value === null ? 'theme' : value).trim().toLowerCase();
    return v === 'on' || v === 'true' ? 'on' : (v === 'off' || v === 'false' ? 'off' : 'theme');
};

Glass.prototype.interactiveMode = function () {
    var current = (meterConfig && meterConfig.current) || {};
    return this.interactiveModeOf(current['touch.interactive']);
};

Glass.prototype.setInteractiveMode = function (value) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var wanted = self.interactiveModeOf(value);
    if (self.interactiveMode() === wanted && meterConfig.current['touch.interactive'] !== undefined) { return { changed: false, interactive: wanted }; }
    meterConfig.current['touch.interactive'] = wanted;
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    self.pushRemoteConfig();
    self.logger.info(id + 'interactive controls: ' + wanted);
    return { changed: true, interactive: wanted };
};

// ---- The views: what the Face tab and Anymote show ---------------------

// `follow`, `face` or `theme` (manager/views.js): the user's choice of
// whether the browser views carry glass-evo's face over the theme.
Glass.prototype.viewsMode = function () {
    return views.modeOf(this.config.get('viewsFace'));
};

Glass.prototype.setViewsMode = function (value) {
    var wanted = views.modeOf(value);
    if (this.viewsMode() === wanted) { return { changed: false, mode: wanted }; }
    this.config.set('viewsFace', wanted);
    this.logger.info(id + 'views: ' + wanted);
    this.tellViews();
    return { changed: true, mode: wanted };
};

// The module a page gets and what goes with it: Glass's own, or the one
// the glass-evo component carries, with the text of the face theme the
// settings name, the user's before a shipped one of its name, for the
// page to put where the face reads it.
Glass.prototype.faceModule = function () {
    var self = this;
    var evo = self.evoComponent();
    var has = !!(evo.installed && evo.face);
    var mode = self.viewsMode();
    var owner = self.screenOwnerState().owner;
    var carries = views.carriesFace(mode, owner, has);
    var theme = carries ? self.faceThemeText() : null;
    return { mode: mode, has: has, owner: owner, face: carries, version: carries ? evo.version : null, theme: theme };
};

// The face theme the settings name, as its name and its text: the user's
// before a shipped one of its name; null where none is named or found.
Glass.prototype.faceThemeText = function () {
    var self = this;
    self.loadConfigs();
    var name = views.themeName(((meterConfig && meterConfig.current) || {})['face.theme']);
    var theme = null;
    if (name) {
        [FACES_DIR, EVO_LOOKS_DIR].some(function (dir) {
            try { theme = { name: name, text: fs.readFileSync(dir + '/' + name + '/face.txt', 'utf8') }; return true; } catch (e) { return false; }
        });
    }
    return theme;
};

// What a remote display that carries a face needs of the player's: whose
// the player's screen is, and the look the settings name. The face's own
// settings travel in the meter configuration.
Glass.prototype.remoteFace = function () {
    var self = this;
    var owner = 'kiosk';
    try { owner = self.screenOwnerState().owner || 'kiosk'; } catch (e) { /* as the kiosk's */ }
    var theme = null;
    try { theme = self.faceThemeText(); } catch (e) { /* the built-in look */ }
    return { owner: owner, theme: theme };
};

// The remotes hear when the screen changes hands: the configuration's
// version covers whose it is, and a remote that follows starts again.
Glass.prototype.tellRemotesOwner = function () {
    var self = this;
    var owner;
    try { owner = self.screenOwnerState().owner; } catch (e) { return; }
    if (self.remoteOwnerTold === owner) { return; }
    var first = self.remoteOwnerTold === undefined;
    self.remoteOwnerTold = owner;
    if (!first) { try { self.updateConfigVersion(); } catch (e) { /* at the next change */ } }
};

// The pages are told when what they should carry changes, so one that is
// open brings the other module: a line down the channel, kept by the feed
// for a page that opens later.
Glass.prototype.tellViews = function () {
    var self = this;
    if (!self.channel) { return; }
    var carries = false;
    try { carries = !!self.faceModule().face; } catch (e) { /* as the theme alone */ }
    if (self.viewsToldFace === carries) { return; }
    self.viewsToldFace = carries;
    self.channel.push({ kind: 'views', face: carries });
};

// ---- Test releases: whether this player is offered pre-releases ----------

Glass.prototype.testReleases = function () {
    return this.config.get('testReleases') === true;
};

Glass.prototype.setTestReleases = function (on) {
    var wanted = on === true;
    if (this.testReleases() === wanted) { return { changed: false, test: wanted }; }
    this.config.set('testReleases', wanted);
    this.logger.info(id + 'releases: test releases ' + (wanted ? 'are offered' : 'are not offered'));
    return { changed: true, test: wanted };
};

// The display launched once: nothing when the player has no screen of its
// own or the display already runs; its lines relayed to the journal; on
// exit, re-armed, restarted or left, as the exit action says.
Glass.prototype.startDisplayOnce = function () {
    var self = this;
                        // A player with no screen of its own serves remote displays and opens no window.
                        if (self.config.get('headless') === true) {
                            return;
                        }
                        // Stepping aside while the kiosk takes the screen: no window until then.
                        if (self.screenYieldUntil && Date.now() < self.screenYieldUntil) {
                            return;
                        }
                        if (self.meterChild && self.meterChild.exitCode === null) {
                            return;
                        }
                        // A display that died at launch time after time: no start until its wait is over.
                        if (self.displayHoldUntil && Date.now() < self.displayHoldUntil) {
                            return;
                        }
                        // The display's lines reach the journal as it writes them,
                        // through the same gate as the plugin's own.
                        var env = self.launchEnv();
                        // After a death at launch the graphics libraries are asked to say what they do, so the journal names what would not load.
                        Object.assign(env, relaunch.diagnosing(Math.max(self.displayDeaths || 0, self.faceFailures || 0)));
                        var child = spawn('/bin/sh', [LaunchScript], { uid: 1000, gid: 1000, env: env, stdio: ['ignore', 'pipe', 'pipe'] });
                        // The face, when glass-evo owns the screen: its failures to start are counted.
                        child.face = !!env.GLASS_BIN;
                        var lastErr = [];
                        var relay = function (chunk, isErr) {
                            String(chunk).split('\n').forEach(function (line) {
                                line = line.trim();
                                if (!line) { return; }
                                if (isErr) { lastErr.push(line); if (lastErr.length > 40) { lastErr.shift(); } }
                                // The display names what it opened the screen with; the Screen tab shows it.
                                var opened = /renderer (\S+) on ([A-Za-z0-9]+)/.exec(line);
                                if (opened) { self.displayRenderer = { renderer: opened[1], driver: opened[2].toLowerCase(), at: Date.now() }; }
                                // The display prefixes its lines as the plugin does; one prefix is enough.
                                self.logger.info(id + line.replace(/^glass: /, ''));
                            });
                        };
                        child.stdout.on('data', function (chunk) { relay(chunk, false); });
                        child.stderr.on('data', function (chunk) { relay(chunk, true); });
                        child.on('error', function (e) { lastErr.push(String(e && e.message ? e.message : e)); });
                        child.on('exit', function (code, signal) {
                            var error = (code === 0) ? null : new Error(signal ? 'signal ' + signal : 'exit ' + code);
                            if (error !== null) {
                                // Why, in a line: the display's last word, the faults said before it, and a library the loader does not know.
                                var why = relaunch.reason(error.message, lastErr, 600);
                                if (relaunch.aboutGraphics(why)) {
                                    try {
                                        var gone = relaunch.missingLibraries(require('child_process').execFileSync('/sbin/ldconfig', ['-p'], { encoding: 'utf8', timeout: 3000, stdio: ['ignore', 'pipe', 'ignore'] }));
                                        if (gone.length) { why += ' | not on this player: ' + gone.join(', ') + ' (sudo apt-get install -y libegl1 libgl1)'; }
                                    } catch (e) { /* the loader's list could not be read: the display's words stand alone */ }
                                }
                                self.logger.error(id + 'the display did not run: ' + why);
                                // Lost to an X server closing under it: no launch for a moment, the watcher brings it back where it belongs.
                                if (/x11 not available|X server/.test(lastErr.join(' '))) { self.screenYieldUntil = Date.now() + 3000; }
                            } else {
                                self.logger.info(id + 'the display ran and left');
                            }
                            if (child.face) {
                                self.faceFailures = screenowner.faceFailures(self.faceFailures || 0, { clean: error === null, ranMs: Date.now() - (child.startedAt || 0), windowMs: METER_CRASH_BACKOFF_MS });
                                self.faceError = error === null ? '' : why;
                                try { self.guardScreen(false); } catch (e) {}
                            }
                            var dismissMarkerPresent = false;
                            try { dismissMarkerPresent = fs.existsSync(dismissFile); } catch (e) {}
                            var action = meterExitAction(error === null, !!self.Timeout, dismissMarkerPresent);
                            try { if (dismissMarkerPresent) fs.removeSync(dismissFile); } catch (e) {}
                            if (self.meterChild === child) {
                                self.meterChild = null;
                                self.displayRenderer = null;
                                // Dead at launch: the same start would die the same way, so each death in a row
                                // waits twice as long, whoever starts the display next (the armed interval, the
                                // screen's watcher). A display the plugin itself ended is no death.
                                var ranMs = Date.now() - (child.startedAt || 0);
                                var ended = signal === 'SIGTERM' || signal === 'SIGKILL' || signal === 'SIGINT';
                                self.displayDeaths = relaunch.deaths(self.displayDeaths || 0, { clean: error === null || ended, ranMs: ranMs });
                                // On a screen of its own the watcher starts it, not the screensaver's delay: the waits begin at a second.
                                var ownScreen = !!child.face;
                                if (!ownScreen) { try { ownScreen = !!self.screenOurs(); } catch (e) { /* as a screensaver */ } }
                                var baseMs = relaunch.base(ownScreen, self.screenTimeoutMs());
                                self.displayHoldUntil = self.displayDeaths ? relaunch.holdUntil(Date.now(), self.displayDeaths, baseMs) : 0;
                                var waitS = Math.round(relaunch.wait(self.displayDeaths, baseMs) / 1000);
                                var inARow = self.displayDeaths > 1 ? ', ' + self.displayDeaths + ' times in a row' : '';
                                if (action === 'rearm') {
                                    clearInterval(self.Timeout);
                                    self.Timeout = setInterval(function () {
                                        self.startDisplayOnce();
                                    }, self.screenTimeoutMs());
                                    self.logger.info(id + 'dismissed by touch, re-armed for ' + (self.screenTimeoutMs() / 1000) + ' s');
                                } else if (action === 'restart') {
                                    if (meterRestartNow(error === null, ranMs)) {
                                        self.startDisplayOnce();
                                    } else {
                                        self.logger.warn(id + 'the display died ' + Math.round(ranMs / 1000) + ' s after launch' + inARow + '; next attempt in ' + waitS + ' s');
                                    }
                                } else if (self.displayDeaths > 0) {
                                    self.logger.warn(id + 'the display died ' + Math.round(ranMs / 1000) + ' s after launch' + inARow + '; not started again for ' + waitS + ' s');
                                }
                            }
                        });
                        child.startedAt = Date.now();
                        self.meterChild = child;
                        // The plain display where glass-evo owns the screen: its component is gone.
                        if (!child.face) { try { self.guardScreen(false); } catch (e) {} }
                        self.displayStartedAt = new Date().toISOString();
};

// The screensaver's start delay in milliseconds, as the settings say.
Glass.prototype.screenTimeoutMs = function () {
    return (parseInt(this.config.get('timeout'), 10) || 0) * 1000;
};

// The face size as kept, normal unless the key says large or car.
function faceSizeOf(value) {
    var size = String(value === undefined || value === null ? 'normal' : value).trim().toLowerCase();
    return ['normal', 'large', 'car'].indexOf(size) === -1 ? 'normal' : size;
}

// A systemd unit's two facts in one process: `show` answers for a unit in
// any state and exits 0, where is-active and is-enabled would cost a
// process each and exit non-zero for the states that matter.
function unitFacts(name) {
    var unit = { state: 'inactive', enabled: false };
    try {
        String(require('child_process').execFileSync('systemctl', ['show', '-p', 'ActiveState', '-p', 'UnitFileState', name], { encoding: 'utf8', timeout: 3000, stdio: ['ignore', 'pipe', 'ignore'] }))
            .split('\n').forEach(function (line) {
                var kv = /^(ActiveState|UnitFileState)=(.*)$/.exec(line.trim());
                if (kv && kv[1] === 'ActiveState') { unit.state = kv[2] || 'inactive'; }
                if (kv && kv[1] === 'UnitFileState') { unit.enabled = kv[2] === 'enabled'; }
            });
    } catch (e) { /* no systemd, or none such unit: inactive */ }
    return unit;
}

// Wait for a condition, polled every half second, up to a limit; answers
// with whether it came true.
function waitFor(test, ms) {
    return new Promise(function (resolve) {
        var started = Date.now();
        (function poll() {
            if (test() || Date.now() - started > ms) { return resolve(test()); }
            setTimeout(poll, 500);
        })();
    });
}

// The fact of the screen, read afresh and cheaply: an X server up, the
// kiosk unit's state and whether it is enabled at boot, the two plugins
// that bring a kiosk, and whether a panel is connected (the DRM status
// files alone, no probe).
Glass.prototype.screenFact = function () {
    var self = this;
    var unit = unitFacts('volumio-kiosk');
    // A plugin brings the kiosk when it runs or is starting; its enabled
    // flag alone counts only in the first two minutes after this plugin
    // started, when plugins come up one after another at boot. Volumio's
    // own toggle sets the flag and the status apart: a flag set without a
    // start brings nothing until the next boot.
    var pm = self.commandRouter.pluginManager;
    var status = function (name) { try { return String(pm.config.get('user_interface.' + name + '.status') || ''); } catch (e) { return ''; } };
    var flagged = function (name) { try { return pm.isEnabled('user_interface', name) === true; } catch (e) { return false; } };
    var booting = Date.now() - (self.pluginStartedAt || 0) < 120000;
    var enabled = function (name) { var st = status(name); return st === 'STARTED' || st === 'STARTING' || (booting && flagged(name)); };
    var panel = false;
    try {
        panel = fs.readdirSync('/sys/class/drm').filter(function (n) { return /^card\d+-/.test(n) && !/Writeback/.test(n); }).some(function (n) {
            try { return fs.readFileSync('/sys/class/drm/' + n + '/status', 'utf8').trim() === 'connected'; } catch (e) { return false; }
        });
    } catch (e) {}
    // The X server brought up for the face: asked after only where a take has linked its unit.
    var ownX = false;
    try { ownX = fs.existsSync('/etc/systemd/system/' + screenowner.OWN_X_UNIT + '.service') && unitFacts(screenowner.OWN_X_UNIT).state === 'active'; } catch (e) {}
    return {
        xserver: fs.existsSync('/tmp/.X11-unix/X0'),
        ownX: ownX,
        kiosk: unit.state,
        kioskEnabled: unit.enabled,
        touchDisplay: enabled('touch_display'),
        displayConfiguration: enabled('display_configuration'),
        panel: panel
    };
};

// A driver key set through the choice the Screen tab offered up to 0.7.82
// goes back to Auto: the screen is drawn by fact now.
Glass.prototype.migrateScreenDriver = function () {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return; }
    var key = String(meterConfig.current['screen.driver'] || 'auto').trim().toLowerCase();
    if (key !== 'kmsdrm' && key !== 'x11') { return; }
    meterConfig.current['screen.driver'] = 'auto';
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    self.logger.info(id + 'screen: drawn by fact now; the driver key ' + key + ' set back to auto');
};

// Whether the screen is Glass's by fact: free, with the driver key not
// forcing X or Wayland. Then the display draws on the screen itself and
// is the only thing that must be on it.
Glass.prototype.screenOurs = function (fact) {
    this.loadConfigs();
    var cur = (meterConfig && meterConfig.current) || {};
    var key = String(cur['screen.driver'] || 'auto').trim().toLowerCase();
    if (key === 'x11' || key === 'wayland') { return false; }
    fact = fact || this.screenFact();
    return screenprobe.screenFree(fact) || screenprobe.screenOwn(fact);
};

// The watcher of the screen: every two seconds the fact is read again.
// Free, the display stays up whether or not the player plays, started now
// and again whenever it is found gone, the console never shown. Not free
// while the display draws on the screen itself, the kiosk wants the
// screen: the display steps aside at once, and the kiosk unit is started
// again if it failed against the display in the meantime, so the player's
// interface is never lost to the screensaver.
Glass.prototype.watchScreen = function () {
    var self = this;
    if (self.screenWatcher) { clearTimeout(self.screenWatcher); self.screenWatcher = null; }
    self.screenWasFree = null;
    var tick = function () {
        var fact;
        try { fact = self.screenFact(); } catch (e) { return; }
        // Glass's own: free, or an X server brought up for the face with no kiosk on it.
        var free = screenprobe.screenFree(fact) || screenprobe.screenOwn(fact);
        var running = !!(self.meterChild && self.meterChild.exitCode === null);
        if (free) {
            if (self.screenWasFree !== true) {
                self.logger.info(id + (fact.ownX ? 'screen: ours through an X server of its own (no kiosk): the display draws on it and stays up' : 'screen: free (no kiosk, no X): the display draws on it and stays up'));
                // The kiosk's X may still be closing: no launch for a moment, so none is tried against it.
                if (self.screenWasFree === false) { self.screenYieldUntil = Date.now() + 3000; }
            }
            if (!running && self.screenOurs(fact)) {
                try { fs.writeFileSync(runFlag, ''); } catch (e) {}
                self.startDisplayOnce();
            }
        } else {
            if (self.screenWasFree === true) {
                self.logger.info(id + 'screen: the kiosk wants it, the display steps aside');
                self.steppedAsideAt = Date.now();
                // A unit already failed before has nothing to do with the display: only a failure from now on is ours to repair.
                self.kioskFailedBefore = fact.kiosk === 'failed';
            }
            if (running && self.displayRenderer && self.displayRenderer.driver === 'kmsdrm') {
                self.screenYieldUntil = Date.now() + 15000;
                try { if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); } } catch (e) {}
            }
            // The kiosk tried while the display held the screen: start it again, once.
            if (!running && fact.kiosk === 'failed' && !self.kioskFailedBefore && (fact.touchDisplay || fact.kioskEnabled) && self.steppedAsideAt && Date.now() - self.steppedAsideAt < 60000) {
                self.steppedAsideAt = 0;
                self.logger.info(id + 'screen: the kiosk failed against the display, starting it again');
                exec('/usr/bin/sudo -n /bin/systemctl restart volumio-kiosk', { uid: 1000, gid: 1000 }, function (error) { if (error) { self.logger.warn(id + 'screen: the kiosk did not start: ' + error.message); } });
            }
        }
        // What the browser views carry follows the screen's owner, the
        // component being here, and the user's choice: told when it changes.
        try { self.tellViews(); } catch (e) { /* told at the next look */ }
        try { self.tellRemotesOwner(); } catch (e) { /* told at the next look */ }
        // The pages are told whether the display stays when the player
        // stands still: once after the start, whether or not the player has
        // stopped since (a page opened on a player that already stood still
        // knew nothing of it), and again when the screen changes hands.
        if (self.channel) {
            var stays = false;
            try { stays = !!self.screenOurs(fact); } catch (e) { /* as a display that leaves */ }
            if (self.persistToldStays !== stays) {
                var told = self.channel.persist || { mode: '', seconds: 0, startedAt: 0 };
                try { self.pushPersist(told.mode, told.seconds, told.startedAt); } catch (e) { /* told at the next stop */ }
            }
        }
        self.screenWasFree = free;
    };
    // Two seconds while the display draws on the screen itself, where it
    // must step aside fast; five otherwise, where nothing is urgent.
    var again = function () {
        tick();
        var period = self.displayRenderer && self.displayRenderer.driver === 'kmsdrm' ? 2000 : 5000;
        self.screenWatcher = setTimeout(again, period);
    };
    again();
};

// ---- The persist period, as a line -------------------------------------

// The display learns the persist countdown from the persist file; a
// browser face cannot read it, so the same goes down the lines: the mode,
// the seconds, and when the period began (epoch ms). Cleared with an
// empty mode. `stays` says whether the display is still on the screen
// when the period is over: it is where the screen is its own (no kiosk, or
// glass-evo holding it), and a page then has no leaving to tell of.
Glass.prototype.pushPersist = function (mode, seconds, startedAt) {
    var self = this;
    if (!self.channel) { return; }
    var stays = false;
    try { stays = !!self.screenOurs(); } catch (e) { /* not known: as a display that leaves */ }
    var line = { kind: 'persist', mode: String(mode || ''), seconds: parseInt(seconds, 10) || 0, startedAt: parseInt(startedAt, 10) || 0, stays: stays };
    self.persistToldStays = stays;
    self.channel.persist = line;
    self.channel.push(line);
};

// ---- The screen: what draws the window, and the picture's turn ----------

// The values the display reads: `screen.driver` (auto, x11, wayland or
// kmsdrm) and `screen.rotation` (0, 90, 180 or 270), and whether the
// kiosk holds the screen now.
Glass.prototype.screenSettings = function () {
    var self = this;
    self.loadConfigs();
    var cur = (meterConfig && meterConfig.current) || {};
    var driver = String(cur['screen.driver'] || 'auto').trim().toLowerCase();
    if (['auto', 'x11', 'wayland', 'kmsdrm'].indexOf(driver) === -1) { driver = 'auto'; }
    var rotation = parseInt(cur['screen.rotation'], 10);
    if ([90, 180, 270].indexOf(rotation) === -1) { rotation = 0; }
    var pointer = String(cur['screen.pointer'] || 'auto').trim().toLowerCase();
    if (['auto', 'show', 'hide'].indexOf(pointer) === -1) { pointer = 'auto'; }
    var found = self.screenProbe(false);
    var fact = self.screenFact();
    var seen = self.graphicsSeen();
    var running = !!(self.meterChild && self.meterChild.exitCode === null);
    return {
        driver: driver,
        rotation: rotation,
        pointer: pointer,
        pointerShown: String(cur['screen.pointer.shown']).toLowerCase() === 'true',
        faceSize: faceSizeOf(cur['face.size']),
        kioskActive: fact.kiosk === 'active',
        free: self.screenOurs(fact),
        ownX: screenprobe.screenOwn(fact),
        fact: fact,
        now: {
            display: { running: running, driver: running && self.displayRenderer ? self.displayRenderer.driver : null, renderer: running && self.displayRenderer ? self.displayRenderer.renderer : null },
            wouldDraw: screenprobe.wouldDraw(fact)
        },
        owner: (function () {
            // Whether a take is held back: the screen would be drawn on itself, and the graphics for that do not work.
            var o = self.screenOwnerState();
            o.blocked = graphics.blocksTake(o.mode, seen);
            return o;
        })(),
        graphics: seen,
        views: (function () { var m = self.faceModule(); return { mode: m.mode, has: m.has, face: m.face }; })(),
        probe: found,
        touch: self.touchSettings()
    };
};

// ---- Graphics: whether the screen can be drawn on itself ----------------

// The display's own probe of the system's graphics, the kiosk's X log and
// Mesa's versions (manager/graphics.js). Asked afresh before a take; kept a
// minute for the Screen tab and the status sheet, which read what was last
// found and start a new look when that is old. The probe is Glass's own
// binary's: the libraries are the system's, whichever display holds the
// screen.
Glass.prototype.checkGraphics = function () {
    var self = this;
    if (self.graphicsChecking) { return self.graphicsChecking; }
    self.graphicsChecking = graphics.check({ bin: PluginPath + '/bin/' + self.volumioArch() + '/glass', uid: 1000, gid: 1000 })
        .then(function (found) {
            self.graphicsChecking = null;
            var told = graphics.summary(found);
            var before = self.graphicsFound ? graphics.summary(self.graphicsFound) : null;
            // Said once, and again when it changes: a line for the journal a report carries.
            if (!before || before.ok !== told.ok || before.reason !== told.reason || before.x.state !== told.x.state) {
                var word = !told.asked ? 'not asked' : !told.applies ? 'no screen the kernel drives' : told.ok ? 'works' : 'DOES NOT WORK';
                self.logger[told.asked && told.applies && told.ok === false ? 'warn' : 'info'](id + 'graphics: drawing on the screen itself: ' + word + (told.reason ? ' (' + told.reason + ')' : '') +
                    '; the kiosk\'s X server: ' + told.x.state + (told.x.detail ? ' (' + told.x.detail + ')' : '') +
                    '; Mesa ' + (told.mesa.agree ? (told.mesa.version || 'not found') : 'parts of different versions: ' + JSON.stringify(told.mesa.versions)));
            }
            self.graphicsFound = found;
            return found;
        });
    return self.graphicsChecking;
};

// What was last found, for the pages; a new look is started when there is
// none or it is over a minute old.
Glass.prototype.graphicsSeen = function () {
    var self = this;
    if (!self.graphicsFound || Date.now() - self.graphicsFound.at > 60000) {
        try { self.checkGraphics().catch(function () { self.graphicsChecking = null; }); } catch (e) { /* at the next look */ }
    }
    return graphics.summary(self.graphicsFound);
};

// What the player has for a screen, read from the kernel and the system
// (manager/screenprobe.js), kept for a few seconds between readers.
Glass.prototype.screenProbe = function (fresh) {
    var self = this;
    if (!fresh && self.probeHeld && Date.now() - self.probeAt < 5000) { return self.probeHeld; }
    self.probeHeld = screenprobe.gather();
    self.probeAt = Date.now();
    return self.probeHeld;
};

// The touch mapping: how a finger's share of the panel is set right
// before it becomes a pixel. `auto` leaves it as the panel reports;
// `overrides` composes a swap and flips; `calibrated` keeps the matrix
// the display found. The display reads `touch.matrix` alone.
var IDENTITY_MATRIX = [1, 0, 0, 0, 1, 0];
function composeOverrides(swap, flipX, flipY) {
    // Applied in this order: swap, then flip X, then flip Y, each on a share in 0..1.
    var m = IDENTITY_MATRIX.slice();
    var mul = function (a, b) { // a after b
        return [a[0] * b[0] + a[1] * b[3], a[0] * b[1] + a[1] * b[4], a[0] * b[2] + a[1] * b[5] + a[2],
                a[3] * b[0] + a[4] * b[3], a[3] * b[1] + a[4] * b[4], a[3] * b[2] + a[4] * b[5] + a[5]];
    };
    if (swap) m = mul([0, 1, 0, 1, 0, 0], m);
    if (flipX) m = mul([-1, 0, 1, 0, 1, 0], m);
    if (flipY) m = mul([1, 0, 0, 0, -1, 1], m);
    return m;
}
function matrixText(m) { return m.map(function (v) { return String(Math.round(v * 100000) / 100000); }).join(','); }
function parseMatrixText(text) {
    var n = String(text || '').split(/[,\s]+/).filter(Boolean).map(Number);
    return n.length === 6 && n.every(isFinite) ? n : IDENTITY_MATRIX.slice();
}

Glass.prototype.touchSettings = function () {
    var self = this;
    var cur = (meterConfig && meterConfig.current) || {};
    var mapping = String(cur['touch.mapping'] || 'auto').trim().toLowerCase();
    if (['auto', 'overrides', 'calibrated'].indexOf(mapping) === -1) { mapping = 'auto'; }
    var truthy = function (v) { return String(v).toLowerCase() === 'true'; };
    return {
        mapping: mapping,
        swap: truthy(cur['touch.swap']),
        flipX: truthy(cur['touch.flip.x']),
        flipY: truthy(cur['touch.flip.y']),
        matrix: parseMatrixText(cur['touch.matrix']),
        calibration: self.calibration || { state: 'none' }
    };
};

// Set the mapping and the overrides; the matrix follows (a calibrated
// matrix is kept as it is), and the display starts again with it.
Glass.prototype.setTouchSettings = function (data) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var now = self.touchSettings();
    var mapping = data.mapping === undefined ? now.mapping : String(data.mapping).trim().toLowerCase();
    if (['auto', 'overrides', 'calibrated'].indexOf(mapping) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var swap = data.swap === undefined ? now.swap : (data.swap === true || data.swap === 'true');
    var flipX = data.flipX === undefined ? now.flipX : (data.flipX === true || data.flipX === 'true');
    var flipY = data.flipY === undefined ? now.flipY : (data.flipY === true || data.flipY === 'true');
    var calibrated = String(meterConfig.current['touch.calibrated'] || '');
    if (mapping === 'calibrated' && !calibrated) { return { error: 'GLASS.MANAGER_TOUCH_NOT_CALIBRATED' }; }
    var matrix = mapping === 'auto' ? IDENTITY_MATRIX : mapping === 'overrides' ? composeOverrides(swap, flipX, flipY) : parseMatrixText(calibrated);
    var wanted = { 'touch.mapping': mapping, 'touch.swap': swap ? 'True' : 'False', 'touch.flip.x': flipX ? 'True' : 'False', 'touch.flip.y': flipY ? 'True' : 'False', 'touch.matrix': matrixText(matrix) };
    var changed = false;
    Object.keys(wanted).forEach(function (k) {
        if (String(meterConfig.current[k]) !== wanted[k]) { meterConfig.current[k] = wanted[k]; changed = true; }
    });
    if (changed) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        self.logger.info(id + 'touch: mapping ' + mapping + ', matrix ' + wanted['touch.matrix']);
    }
    return { ok: true, changed: changed };
};

// Ask the display to calibrate: it shows targets, reads the fingers and
// answers with the matrix as a `calibration` command.
Glass.prototype.startCalibration = function () {
    var self = this;
    if (!self.channel || !self.channel.remotes || self.channel.connections === 0) { /* the display connects when it runs */ }
    self.calibration = { state: 'waiting', at: Date.now(), points: 5 };
    self.channel.push({ kind: 'calibrate', points: 5 });
    setTimeout(function () {
        if (self.calibration && self.calibration.state === 'waiting' && Date.now() - self.calibration.at > 118000) { self.calibration = { state: 'failed', error: 'timeout', at: Date.now() }; }
    }, 120000);
    return { ok: true, calibration: self.calibration };
};

// The display's answer: the matrix, or why there is none. A matrix is
// kept as the calibrated one and put to use at once.
Glass.prototype.takeCalibration = function (value) {
    var self = this;
    var v = value && typeof value === 'object' ? value : {};
    if (v.error || !Array.isArray(v.matrix) || v.matrix.length !== 6) {
        self.calibration = { state: 'failed', error: String(v.error || 'unfit'), worst: typeof v.error_px === 'number' ? v.error_px : null, at: Date.now() };
        self.logger.warn(id + 'touch: calibration failed: ' + self.calibration.error + (self.calibration.worst !== null ? ', worst ' + self.calibration.worst + ' px' : ''));
        return;
    }
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { self.calibration = { state: 'failed', error: 'no-config', at: Date.now() }; return; }
    var text = matrixText(v.matrix.map(Number));
    meterConfig.current['touch.calibrated'] = text;
    meterConfig.current['touch.mapping'] = 'calibrated';
    meterConfig.current['touch.matrix'] = text;
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    self.calibration = { state: 'done', error: null, worst: typeof v.error_px === 'number' ? v.error_px : null, at: Date.now() };
    self.logger.info(id + 'touch: calibrated, matrix ' + text + (self.calibration.worst !== null ? ', worst ' + self.calibration.worst + ' px' : ''));
};

// The pointer as the display reads it, resolved from the choice and what
// the player has; written when it differs, without a restart: the display
// starts with it next time.
Glass.prototype.refreshPointerShown = function () {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return false; }
    var cur = meterConfig.current || {};
    var choice = String(cur['screen.pointer'] || 'auto').trim().toLowerCase();
    var shown = screenprobe.pointerShown(choice, self.screenProbe(true)) ? 'True' : 'False';
    if (String(cur['screen.pointer.shown']) === shown) { return false; }
    meterConfig.current['screen.pointer.shown'] = shown;
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    self.logger.info(id + 'screen: pointer ' + choice + ', shown ' + shown);
    return true;
};

// Whether the kiosk, the Touch Display plugin's X session and browser,
// holds the screen: a display drawing through KMS/DRM would fight it.
Glass.prototype.kioskActive = function () {
    try {
        var out = require('child_process').execFileSync('systemctl', ['is-active', 'volumio-kiosk'], { encoding: 'utf8', timeout: 3000, stdio: ['ignore', 'pipe', 'ignore'] });
        return String(out).trim() === 'active';
    } catch (e) {
        return false;
    }
};

// The same, set: KMS/DRM is refused while the kiosk holds the screen; the
// meter configuration is written and the display starts again with it.
Glass.prototype.setScreenSettings = function (data) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var now = self.screenSettings();
    // The driver is drawn by fact since 0.7.83; the key stays as it is.
    var driver = now.driver;
    var rotation = data.rotation === undefined ? now.rotation : parseInt(data.rotation, 10);
    if ([0, 90, 180, 270].indexOf(rotation) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var pointer = data.pointer === undefined ? now.pointer : String(data.pointer).trim().toLowerCase();
    if (['auto', 'show', 'hide'].indexOf(pointer) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var faceSize = data.faceSize === undefined ? now.faceSize : String(data.faceSize).trim().toLowerCase();
    if (['normal', 'large', 'car'].indexOf(faceSize) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var shown = screenprobe.pointerShown(pointer, now.probe) ? 'True' : 'False';
    var wanted = { 'screen.driver': driver, 'screen.rotation': String(rotation), 'screen.pointer': pointer, 'screen.pointer.shown': shown, 'face.size': faceSize };
    var changed = false;
    Object.keys(wanted).forEach(function (k) {
        if (String(meterConfig.current[k]) !== wanted[k]) { meterConfig.current[k] = wanted[k]; changed = true; }
    });
    if (changed) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        self.logger.info(id + 'screen: drawn by ' + driver + ', rotation ' + rotation + ', pointer ' + pointer + ' (shown ' + shown + '), face ' + faceSize);
        // The watcher reads the fact again and brings the display back as it is now.
        setTimeout(function () { try { self.watchScreen(); } catch (e) {} }, 1500);
    }
    return { ok: true, changed: changed };
};

// ---- The screen's owner: the kiosk, or glass-evo ------------------------

// glass-evo as a component the Manager keeps under the data folder: a
// manifest and the binaries by architecture. Available means the manifest
// and the binary for this player's architecture are there.
const EVO_DIR = DATA_DIR + '/evo';
// Face themes: a folder each, with the theme's text in face.txt. The
// user's own, and the looks the glass-evo component ships.
const FACES_DIR = DATA_DIR + '/faces';
const EVO_LOOKS_DIR = EVO_DIR + '/themes';
// The unit of the X server brought up for the face, shipped with the plugin and enabled by this path on a take.
const OWN_X_UNIT_FILE = PluginPath + '/' + screenowner.OWN_X_UNIT + '.service';

// The face's own settings, as the display's configuration has them: the
// look's keys by name, the face themes installed, and the plugin's word on
// whether frost over a moving theme suits this board.
Glass.prototype.faceSettings = function () {
    var self = this;
    self.loadConfigs();
    // The looks to choose from: a face theme of the user's stands before a
    // shipped one of its name, as it does for the face itself.
    var looks = [];
    [[FACES_DIR, false], [EVO_LOOKS_DIR, true]].forEach(function (place) {
        var names = [];
        try { names = fs.readdirSync(place[0]).sort(); } catch (e) { /* none there */ }
        names.forEach(function (name) {
            if (name[0] === '.' || looks.some(function (l) { return l.name === name; })) { return; }
            try {
                looks.push({ name: name, shipped: place[1], keys: facelook.themeKeys(fs.readFileSync(place[0] + '/' + name + '/face.txt', 'utf8')) });
            } catch (e) { /* a folder with no theme in it */ }
        });
    });
    // The built-in look, as the component writes it out; none from one
    // that does not.
    var builtIn = {};
    try { builtIn = facelook.themeKeys(fs.readFileSync(EVO_DIR + '/face.txt', 'utf8')); } catch (e) { /* not written out */ }
    // What the theme on show brings for the face: a face.txt beside its
    // meters.txt, which the face lays over the look chosen here; none
    // where the theme has no such file.
    var themeLook = null;
    var theme = self.activeTheme();
    if (safeFolderName(theme)) {
        try { themeLook = { folder: theme, keys: facelook.themeKeys(fs.readFileSync(base_folder_P + theme + '/face.txt', 'utf8')) }; } catch (e) { /* the theme brings none */ }
    }
    return {
        settings: facelook.settingsOf(meterConfig && meterConfig.current),
        builtIn: builtIn,
        looks: looks,
        themeLook: themeLook,
        frostSuits: facelook.frostSuits(self.boardInfo().class)
    };
};

// Changes to the configuration's face keys, written and the display
// started again so the face reads them; says whether anything changed.
Glass.prototype.applyFaceChanges = function (changes, quietly) {
    var self = this;
    var keys = Object.keys(changes);
    if (!keys.length) { return false; }
    keys.forEach(function (key) {
        if (changes[key] === null) { delete meterConfig.current[key]; } else { meterConfig.current[key] = changes[key]; }
    });
    fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
    try { self.updateConfigVersion(); } catch (e) {}
    if (!quietly && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    return true;
};

Glass.prototype.setFaceSettings = function (set, reset) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var planned = facelook.planAll(meterConfig.current, set, reset);
    if (planned.error) { return planned; }
    var changes = planned.changes;
    var changed = self.applyFaceChanges(changes);
    if (changed) { self.logger.info(id + 'face: ' + Object.keys(changes).map(function (k) { return k + (changes[k] === null ? ' removed' : ' = ' + changes[k]); }).join(', ')); }
    return Object.assign({ ok: true, changed: changed }, self.faceSettings());
};

// The plugin's word on frost for this board, kept beside the face's
// settings so the face can follow it where the user has not decided.
Glass.prototype.noteFrostSuits = function () {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !meterConfig.current || !fs.existsSync(MeterConfigFile)) { return; }
    var suits = facelook.frostSuits(self.boardInfo().class) ? 'true' : 'false';
    if (String(meterConfig.current['face.frost.suits']) !== suits) { self.applyFaceChanges({ 'face.frost.suits': suits }, true); }
};
const REGISTER_FILE = DATA_DIR + '/screen-owner.json';

Glass.prototype.evoComponent = function () {
    return component.installedAt(EVO_DIR, this.volumioArch());
};

// The register: who owns the screen, since when, what was found, and what
// the take changed in order, each with what it was before; the way back
// adds when it happened and what it restored.
Glass.prototype.readRegister = function () {
    try { return JSON.parse(fs.readFileSync(REGISTER_FILE, 'utf8')); } catch (e) { return null; }
};

Glass.prototype.writeRegister = function (register) {
    try { fs.mkdirSync(DATA_DIR, { recursive: true }); } catch (e) {}
    fs.writeFileSync(REGISTER_FILE, JSON.stringify(register, null, 2));
};

// The kiosk plugins as they stand: installed, their enabled flag and their
// running status, which Volumio keeps apart.
Glass.prototype.kioskPluginStates = function () {
    var self = this;
    var pm = self.commandRouter.pluginManager;
    var out = {};
    screenowner.KIOSK_PLUGINS.forEach(function (name) {
        var enabled = false;
        var status = '';
        try { enabled = pm.isEnabled('user_interface', name) === true; } catch (e) {}
        try { status = String(pm.config.get('user_interface.' + name + '.status') || ''); } catch (e) {}
        out[name] = { installed: fs.existsSync('/data/plugins/user_interface/' + name), enabled: enabled, status: status };
    });
    return out;
};

Glass.prototype.kioskUnitStates = function () {
    var out = {};
    screenowner.KIOSK_UNITS.forEach(function (name) { out[name] = unitFacts(name); });
    return out;
};

Glass.prototype.screenOwnerState = function () {
    var self = this;
    var register = self.readRegister();
    var evo = self.evoComponent();
    var mode = null;
    try { mode = self.holdMode(false); } catch (e) {}
    return { owner: screenowner.ownedByEvo(register) ? 'glass-evo' : 'kiosk', evo: evo, register: register, here: screenowner.here(register, evo), holdable: mode !== null, mode: mode };
};

// The component was installed, updated or put back: the board's word on
// frost is kept for it, the face's count of failures starts again, and a
// face on the screen leaves to come back as the new one.
Glass.prototype.componentChanged = function () {
    var self = this;
    try { self.noteFrostSuits(); } catch (e) {}
    self.faceFailures = 0;
    self.displayDeaths = 0;
    self.displayHoldUntil = 0;
    self.guardNotBefore = 0;
    try {
        if (self.screenOwnerState().owner === 'glass-evo' && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    } catch (e) {}
};

// How glass-evo would hold this player's screen: on the screen itself, on
// an X server of its own, or not at all.
Glass.prototype.holdMode = function (fresh) {
    var xInstalled = fs.existsSync('/usr/bin/xinit') && (fs.existsSync('/usr/bin/X') || fs.existsSync('/usr/bin/Xorg'));
    return screenowner.holdMode(this.volumioArch(), this.screenProbe(fresh), xInstalled);
};

// The screen back to the kiosk without being asked, where glass-evo owns it
// and cannot hold it (screenowner.guard has the cases; `stopped` is the
// look a moment after the plugin's stop): the way back as the Manager's
// own, with the reason written into the register for the Screen tab to
// say. Answers with the way back while one runs, else null.
Glass.prototype.guardScreen = function (stopped) {
    var self = this;
    if (self.screenGoingBack) { return self.screenGoingBack; }
    // A way back that did not finish is tried again a minute later, not at every launch.
    if (self.guardNotBefore && Date.now() < self.guardNotBefore) { return null; }
    var state = self.screenOwnerState();
    var enabled = true;
    if (stopped) { try { enabled = self.commandRouter.pluginManager.isEnabled('user_interface', 'glass') === true; } catch (e) {} }
    var reason = screenowner.guard({ owner: state.owner, available: state.evo.available, failures: self.faceFailures || 0, stopped: !!stopped, enabled: enabled });
    if (!reason) { return null; }
    var failure = reason === 'face-failed' ? String(self.faceError || '').slice(0, 600) : '';
    self.logger.warn(id + 'screen owner: glass-evo cannot hold the screen (' + reason + (failure ? ': ' + failure : '') + '); the kiosk gets it back');
    self.faceFailures = 0;
    self.screenGoingBack = self.setScreenOwner('kiosk').then(function (result) {
        self.screenGoingBack = null;
        var reg = self.readRegister();
        if (result && result.ok && reg && reg.gaveBackAt) {
            reg.gaveBackBecause = reason;
            if (failure) { reg.failure = failure; }
            self.writeRegister(reg);
        } else {
            self.guardNotBefore = Date.now() + 60000;
        }
        return result;
    }, function (e) {
        self.screenGoingBack = null;
        self.guardNotBefore = Date.now() + 60000;
        self.logger.warn(id + 'screen owner: the way back did not run: ' + (e && e.message ? e.message : e));
        return null;
    });
    return self.screenGoingBack;
};

// glass-evo takes the screen, or the kiosk gets it back. A take turns the
// kiosk plugins off through their own lifecycles, waits for X to go, then
// stops and disables the units behind the kiosk, recording each change as
// it lands so that a take that fails halfway can still be undone. The way
// back has the display step aside first, so the kiosk's X never starts
// against it, then restores in reverse order only what is still as the
// take left it. The watcher does the rest by fact.
Glass.prototype.setScreenOwner = function (owner, options) {
    var self = this;
    var systemctl = function (args) {
        return new Promise(function (resolve, reject) {
            exec('/usr/bin/sudo -n /bin/systemctl ' + args, { uid: 1000, gid: 1000 }, function (error) { if (error) { reject(error); } else { resolve(); } });
        });
    };
    var chain = function (steps, fn) {
        return steps.reduce(function (p, step) { return p.then(function () { return fn(step); }); }, Promise.resolve());
    };
    var problem = function (e) { return String(e && e.message ? e.message : e); };
    var runTake = function (step) {
        if (step.kind === 'plugin') {
            self.logger.info(id + 'screen owner: ' + step.name + ' off');
            return Promise.resolve(self.commandRouter.disableAndStopPlugin(step.category, step.name));
        }
        self.logger.info(id + 'screen owner: ' + step.name + ' stopped' + (step.was.enabled ? ' and disabled' : ''));
        return systemctl('stop ' + step.name).then(function () { return step.was.enabled ? systemctl('disable ' + step.name) : null; })
            // startx leaves with an error when its X server is stopped under it: the unit is not left standing as failed.
            .then(function () { return systemctl('reset-failed ' + step.name).catch(function () { return null; }); });
    };
    var runBack = function (step) {
        if (step.kind === 'own-x') {
            // Stopped and unlinked whatever its state, then gone before the kiosk starts its own.
            self.logger.info(id + 'screen owner: ' + step.name + ' stopped');
            var quiet = function () { return null; };
            return systemctl('stop ' + step.name).catch(quiet)
                .then(function () { return systemctl('disable ' + step.name).catch(quiet); })
                // xinit leaves with an error when stopped; nothing of the unit is left standing as failed.
                .then(function () { return systemctl('reset-failed ' + step.name).catch(quiet); })
                .then(function () { return waitFor(function () { return !fs.existsSync('/tmp/.X11-unix/X0'); }, 10000); });
        }
        if (step.kind === 'plugin') {
            self.logger.info(id + 'screen owner: ' + step.name + ' on');
            return Promise.resolve(self.commandRouter.enableAndStartPlugin(step.category, step.name));
        }
        self.logger.info(id + 'screen owner: ' + step.name + (step.enable ? ' enabled' : '') + (step.start ? ' started' : ''));
        return (step.enable ? systemctl('enable ' + step.name) : Promise.resolve()).then(function () { return step.start ? systemctl('start ' + step.name) : null; });
    };
    var state = self.screenOwnerState();
    if (owner === 'glass-evo') {
        if (state.owner === 'glass-evo') { return Promise.resolve({ ok: true, changed: false }); }
        if (!state.evo.available) { return Promise.resolve({ error: 'GLASS.MANAGER_OWNER_EVO_ABSENT' }); }
        // Nothing is turned off for a face that would have no screen to draw on.
        var mode = self.holdMode(true);
        if (!mode) { return Promise.resolve({ error: 'GLASS.MANAGER_OWNER_NO_SCREEN' }); }
        // Nothing is turned off either where the screen would be drawn on itself and the graphics for that do
        // not work: asked afresh, said with the reason, and passed over only at the user's own word.
        if (mode === 'kms' && !(options && options.force) && !(options && options.checked)) {
            return self.checkGraphics().then(function (found) {
                var seenNow = graphics.summary(found);
                if (graphics.blocksTake(mode, seenNow)) {
                    self.logger.warn(id + 'screen owner: the take is held back, the graphics to draw on the screen itself do not work: ' + seenNow.reason);
                    return { error: 'GLASS.MANAGER_OWNER_NO_GRAPHICS', message: seenNow.reason };
                }
                return self.setScreenOwner(owner, { checked: true });
            });
        }
        if (options && options.force) { self.logger.warn(id + 'screen owner: the take goes on at the user\'s word, whatever the graphics check said'); }
        var register = { owner: 'glass-evo', takenAt: new Date().toISOString(), evo: state.evo.version, mode: mode, found: self.screenFact(), changes: [] };
        var pluginSteps = screenowner.planTakePlugins(self.kioskPluginStates());
        self.logger.info(id + 'screen owner: glass-evo takes the screen (' + pluginSteps.length + ' plugin' + (pluginSteps.length === 1 ? '' : 's') + ' to turn off)');
        return chain(pluginSteps, function (step) { return runTake(step).then(function () { register.changes.push(step); }); })
            .then(function () { return waitFor(function () { return !fs.existsSync('/tmp/.X11-unix/X0'); }, 20000); })
            // A kiosk unit left "failed" by an earlier fight would read as a failure later; cleared, it reads as what it is.
            .then(function () { return systemctl('reset-failed volumio-kiosk').catch(function () { return null; }); })
            .then(function () {
                var unitSteps = screenowner.planTakeUnits(self.kioskUnitStates());
                return chain(unitSteps, function (step) { return runTake(step).then(function () { register.changes.push(step); }); });
            })
            .then(function () {
                // Where the face draws through X: a plain X server in the kiosk's place, once the kiosk's own has gone.
                if (mode !== 'x') { return null; }
                self.logger.info(id + 'screen owner: an X server of its own for the face (' + screenowner.OWN_X_UNIT + ')');
                return waitFor(function () { return !fs.existsSync('/tmp/.X11-unix/X0'); }, 10000)
                    .then(function () { return systemctl('enable ' + OWN_X_UNIT_FILE); })
                    .then(function () {
                        register.changes.push({ kind: 'own-x', name: screenowner.OWN_X_UNIT, action: 'start' });
                        return systemctl('start ' + screenowner.OWN_X_UNIT);
                    })
                    .then(function () { return waitFor(function () { return fs.existsSync('/tmp/.X11-unix/X0'); }, 20000); })
                    .then(function (up) { if (!up) { throw new Error('the X server for the face did not come up'); } });
            })
            .then(function () {
                self.writeRegister(register);
                self.probeAt = 0;
                // Another display on another screen: what died before the screen changed hands says nothing of it.
                self.displayDeaths = 0;
                self.displayHoldUntil = 0;
                self.faceFailures = 0;
                self.logger.info(id + 'screen owner: glass-evo has the screen; ' + register.changes.length + ' change' + (register.changes.length === 1 ? '' : 's') + ' recorded');
                // A display already on a free screen would stay as it is:
                // it leaves now and comes back as the face.
                try { if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); } } catch (e) {}
                return { ok: true, changed: true };
            }, function (e) {
                register.error = problem(e);
                self.writeRegister(register);
                self.logger.warn(id + 'screen owner: the take did not finish: ' + register.error);
                return { error: 'GLASS.MANAGER_OWNER_FAILED', message: register.error };
            });
    }
    if (owner === 'kiosk') {
        if (state.owner !== 'glass-evo') { return Promise.resolve({ ok: true, changed: false }); }
        var reg = state.register;
        var steps = screenowner.planGiveBack(reg, { plugins: self.kioskPluginStates(), units: self.kioskUnitStates() });
        self.logger.info(id + 'screen owner: the kiosk gets the screen back (' + steps.length + ' to restore); the display steps aside');
        // The screensaver over the kiosk is another display: the face's deaths are not counted against it.
        self.displayDeaths = 0;
        self.displayHoldUntil = 0;
        self.screenYieldUntil = Date.now() + 30000;
        try { if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); } } catch (e) {}
        return waitFor(function () { return !(self.meterChild && self.meterChild.exitCode === null); }, 10000)
            .then(function () { return chain(steps, runBack); })
            .then(function () {
                reg.gaveBackAt = new Date().toISOString();
                reg.restored = steps;
                self.writeRegister(reg);
                self.screenYieldUntil = 0;
                self.probeAt = 0;
                return { ok: true, changed: true };
            }, function (e) {
                self.screenYieldUntil = 0;
                self.logger.warn(id + 'screen owner: the way back did not finish: ' + problem(e));
                return { error: 'GLASS.MANAGER_OWNER_FAILED', message: problem(e) };
            });
    }
    return Promise.resolve({ error: 'GLASS.MANAGER_BAD_REQUEST' });
};

// ---- Car Dash: a day theme and a night theme by the clock ---------------

// The place Car Dash reckons the sun from: the coordinates set by hand,
// else the reference point of the player's time zone from the system's
// zone table, else none.
Glass.prototype.carDashLocation = function () {
    var self = this;
    var lat = parseFloat(self.config.get('carDashLat'));
    var lon = parseFloat(self.config.get('carDashLon'));
    var zone = cardash.zoneLocation(fs);
    if (isFinite(lat) && isFinite(lon)) { return { lat: lat, lon: lon, source: 'manual', zone: zone ? zone.zone : '' }; }
    if (zone && zone.lat !== null) { return { lat: zone.lat, lon: zone.lon, source: 'zone', zone: zone.zone }; }
    return { lat: null, lon: null, source: 'none', zone: zone ? zone.zone : '' };
};

Glass.prototype.carDashInfo = function () {
    var self = this;
    self.loadConfigs();
    var info = {
        enabled: self.config.get('carDashEnabled') === true,
        mode: String(self.config.get('carDashMode') || 'clock') === 'sun' ? 'sun' : 'clock',
        dayTheme: String(self.config.get('carDashDayTheme') || ''),
        nightTheme: String(self.config.get('carDashNightTheme') || ''),
        dayAt: String(self.config.get('carDashDayAt') || '07:00'),
        nightAt: String(self.config.get('carDashNightAt') || '20:00'),
        offsetMin: parseInt(self.config.get('carDashOffsetMin'), 10) || 0,
        lat: String(self.config.get('carDashLat') || ''),
        lon: String(self.config.get('carDashLon') || '')
    };
    info.location = self.carDashLocation();
    var now = new Date();
    // By the sun when asked and possible today; the clock times stand in
    // on a day the sun neither rises nor sets, or with no place known.
    var sun = info.mode === 'sun' && info.location.lat !== null ? cardash.sunSwitches(now, info.location.lat, info.location.lon, info.offsetMin) : null;
    info.sun = sun ? { rise: sun.rise.toISOString(), set: sun.set.toISOString() } : null;
    info.bySun = !!sun;
    var next;
    if (sun) {
        info.period = cardash.periodBySun(now, info.location.lat, info.location.lon, info.offsetMin);
        next = cardash.nextSwitchBySun(now, info.location.lat, info.location.lon, info.offsetMin);
    } else {
        info.period = cardash.periodAt(now, info.dayAt, info.nightAt);
        next = cardash.nextSwitch(now, info.dayAt, info.nightAt);
    }
    info.next = next ? { at: next.at.toISOString(), period: next.period } : null;
    info.active = self.activeTheme();
    return info;
};

// Keep the settings and apply them: `{ enabled, dayTheme, nightTheme, dayAt, nightAt }`.
Glass.prototype.setCarDash = function (data) {
    var self = this;
    self.loadConfigs();
    data = data || {};
    var enabled = data.enabled === true || data.enabled === 'true';
    var dayAt = String(data.dayAt || '07:00').trim();
    var nightAt = String(data.nightAt || '20:00').trim();
    if (cardash.minutesOf(dayAt) === null || cardash.minutesOf(nightAt) === null) { return { error: 'bad-time' }; }
    if (cardash.minutesOf(dayAt) === cardash.minutesOf(nightAt)) { return { error: 'same-time' }; }
    var dayTheme = String(data.dayTheme || '').trim();
    var nightTheme = String(data.nightTheme || '').trim();
    var exists = function (folder) { return safeFolderName(folder) && fs.existsSync(base_folder_P + folder + '/meters.txt'); };
    if ((enabled || dayTheme) && !exists(dayTheme)) { return { error: 'bad-theme' }; }
    if ((enabled || nightTheme) && !exists(nightTheme)) { return { error: 'bad-theme' }; }
    var mode = String(data.mode || 'clock') === 'sun' ? 'sun' : 'clock';
    var latText = String(data.lat === undefined || data.lat === null ? '' : data.lat).trim();
    var lonText = String(data.lon === undefined || data.lon === null ? '' : data.lon).trim();
    if ((latText === '') !== (lonText === '')) { return { error: 'bad-location' }; }
    if (latText !== '') {
        var lat = parseFloat(latText), lon = parseFloat(lonText);
        if (!isFinite(lat) || !isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180) { return { error: 'bad-location' }; }
        latText = String(lat); lonText = String(lon);
    }
    var offset = parseInt(data.offsetMin, 10);
    if (!isFinite(offset)) { offset = 0; }
    if (Math.abs(offset) > 180) { return { error: 'bad-offset' }; }
    self.config.set('carDashLat', latText);
    self.config.set('carDashLon', lonText);
    if (mode === 'sun' && self.carDashLocation().lat === null) { return { error: 'no-location' }; }
    self.config.set('carDashEnabled', enabled);
    self.config.set('carDashMode', mode);
    self.config.set('carDashDayTheme', dayTheme);
    self.config.set('carDashNightTheme', nightTheme);
    self.config.set('carDashDayAt', dayAt);
    self.config.set('carDashNightAt', nightAt);
    self.config.set('carDashOffsetMin', offset);
    self.logger.info(id + 'car dash: ' + (enabled ? 'on, day ' + dayTheme + ', night ' + nightTheme + (mode === 'sun' ? ', by the sun' + (offset ? ' with ' + offset + ' min' : '') : ', from ' + dayAt + ' and ' + nightAt) : 'off'));
    self.armCarDash();
    return { changed: true };
};

// Put the period's theme on show when it is not, and set the timer for
// the next switch; called at start, after every change, and by the timer.
Glass.prototype.armCarDash = function () {
    var self = this;
    if (self.carDashTimer) { clearTimeout(self.carDashTimer); self.carDashTimer = null; }
    var info = self.carDashInfo();
    if (!info.enabled || !info.period) { return; }
    var wanted = info.period === 'day' ? info.dayTheme : info.nightTheme;
    if (wanted && wanted !== info.active) {
        var result = self.activateTheme(wanted);
        self.logger.info(id + 'car dash: the ' + info.period + ' theme ' + wanted + (info.bySun ? ' (by the sun)' : '') + (result.error ? ' was not put on show: ' + result.error : ' is on show'));
    }
    if (!info.next) { return; }
    // At most a day at a time: the sun's times are reckoned afresh each day.
    var wait = Math.min(Math.max(Date.parse(info.next.at) - Date.now(), 1000), 24 * 3600 * 1000) + 500;
    self.carDashTimer = setTimeout(function () { self.carDashTimer = null; self.armCarDash(); }, wait);
};

// Kilobytes under a folder, and free on the filesystem holding a path.
function folderKb(dir) {
    var total = 0;
    var visit = function (d) {
        var names = [];
        try { names = fs.readdirSync(d); } catch (e) { return; }
        names.forEach(function (name) {
            var full = d + '/' + name;
            var stat;
            try { stat = fs.lstatSync(full); } catch (e) { return; }
            if (stat.isDirectory()) { visit(full); } else if (stat.isFile()) { total += stat.size; }
        });
    };
    visit(dir);
    return Math.ceil(total / 1024);
}
function freeKb(path) {
    try { var s = fs.statfsSync(path); return Math.floor(s.bavail * s.bsize / 1024); } catch (e) { return -1; }
}

// A configuration that still names the old plugin's theme folders (a
// player that took its themes over in place) has them copied into Glass's
// own folders once, and is pointed there; the old folders are left as they
// were, for a return to that plugin. Nothing happens when the free space
// would not hold the copy. Resolves, true after a copy, when the manager
// may start on the folders the configuration names.
Glass.prototype.adoptLegacyThemes = function () {
    var self = this;
    self.loadConfigs();
    var inOld = function (root) { return String(root || '').indexOf(LEGACY_DATA + '/') === 0; };
    var jobs = [];
    if (inOld(base_folder_P)) { jobs.push({ from: base_folder_P.replace(/\/$/, ''), to: DATA_DIR + '/templates' }); }
    if (inOld(base_folder_S)) { jobs.push({ from: base_folder_S.replace(/\/$/, ''), to: DATA_DIR + '/templates_spectrum' }); }
    jobs = jobs.filter(function (j) { try { return fs.statSync(j.from).isDirectory(); } catch (e) { return false; } });
    if (!jobs.length) { return Promise.resolve(false); }
    var need = jobs.reduce(function (sum, j) { return sum + folderKb(j.from); }, 0);
    fs.ensureDirSync(DATA_DIR);
    var free = freeKb(DATA_DIR);
    if (free < 0 || free < need + 65536) {
        self.logger.warn(id + 'themes: ' + LEGACY_PLUGIN + "'s folders stay in use: " + Math.round(need / 1024) + ' MB to copy, ' + Math.round(Math.max(free, 0) / 1024) + ' MB free');
        return Promise.resolve(false);
    }
    self.logger.info(id + 'themes: copying ' + jobs.map(function (j) { return j.from; }).join(' and ') + ' into ' + DATA_DIR + ' (' + Math.round(need / 1024) + ' MB)');
    var started = Date.now();
    return jobs.reduce(function (chain, j) {
        return chain.then(function () { return fs.copy(j.from, j.to, { overwrite: true, preserveTimestamps: true }); });
    }, Promise.resolve()).then(function () {
        // A folder added to the old tree while the copy ran comes over too.
        jobs.forEach(function (j) {
            fs.readdirSync(j.from).forEach(function (name) {
                if (name.indexOf('.') !== 0 && !fs.existsSync(j.to + '/' + name)) { fs.copySync(j.from + '/' + name, j.to + '/' + name, { preserveTimestamps: true }); }
            });
        });
        // The switch: the configurations name Glass's folders from now on.
        self.loadConfigs();
        if (meterConfig && inOld(base_folder_P)) {
            meterConfig.current['base.folder'] = DATA_DIR + '/templates';
            fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        }
        if (spectrum_config && inOld(base_folder_S)) {
            spectrum_config.current['base.folder'] = DATA_DIR + '/templates_spectrum';
            fs.writeFileSync(SpectrumConfigFile, ini.stringify(spectrum_config, { whitespace: true }));
        }
        self.loadConfigs();
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        self.pushRemoteConfig();
        self.logger.info(id + 'themes: ' + LEGACY_PLUGIN + "'s themes copied into " + DATA_DIR + ' in ' + Math.round((Date.now() - started) / 1000) + ' s; the old folders are left as they were');
        return true;
    }).catch(function (e) {
        self.logger.error(id + 'themes: copying from ' + LEGACY_PLUGIN + ': ' + (e && e.message ? e.message : e));
        return false;
    });
};

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
            var restored = self.backupRestore(backups[0].name);
            if (restored.error) {
                self.logger.warn(id + 'settings import: backup ' + backups[0].name + ': ' + restored.error);
            }
            imported = !restored.error;
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
            C('positionFit').value = String(meterConfig.current['position.fit']).toLowerCase() === 'true' || meterConfig.current['position.type'] == 'fit';
            if (meterConfig.current['position.type'] == 'center' || meterConfig.current['position.type'] == 'fit') {
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
            var interactiveMode = self.interactiveMode();
            C('interactiveMode').value = { value: interactiveMode, label: self.commandRouter.getI18nString('GLASS.INTERACTIVE_' + interactiveMode.toUpperCase()) };
            if (self.config.get('headless') === true) {
                C('displayOutput').value.value = 'none';
                C('displayOutput').value.label = self.commandRouter.getI18nString('GLASS.DISPLAY_OUTPUT_NONE');
            } else {
                C('displayOutput').value.value = self.config.get('displayOutput');
                C('displayOutput').value.label = 'Display=' + self.config.get('displayOutput');
            }
            var formatTypeMode = meterConfig.current['playinfo.type.mode'] || 'icon';
            if (formatTypeMode !== 'icon' && formatTypeMode !== 'text' && formatTypeMode !== 'both') {
                formatTypeMode = 'icon';
            }
            pick('formatTypeMode', formatTypeMode);

            // The manager: where themes, artwork and backups are managed.
            var managerUrl = self.managerUrl();
            C('managerPort').value = parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT;
            C('managerHost').value = String(self.config.get('managerHost') || '');
            C('managerHost').attributes[0].placeholder = self.managerDefaultHost();
            // The UI opens an openUrl button in a new tab, always; the one
            // button type it navigates in place is the oauth one, which sets
            // the window's location to its performer URL (with two query
            // parameters the page ignores). So "open here" goes that way,
            // to Volumio's own iframe page holding the manager.
            C('managerOpen').onClick.performerUrl = 'http://' + self.managerHost() + '/iframe-page/' + managerUrl.replace(/\//g, '~2F');
            C('managerOpenTab').onClick.url = managerUrl;
            C('managerOpenTab').doc = self.commandRouter.getI18nString('GLASS.MANAGER_OPEN_TAB_DOC') + ' ' + managerUrl;

            // Themes: the active theme, the tag rules and whether themes stay.
            var files = [];
            try { files = fs.readdirSync(base_folder_P); } catch (e) {}
            files.forEach(function (file) {
                var stat;
                try { stat = fs.statSync(base_folder_P + file); } catch (e) { return; }
                if (!stat.isDirectory() || file.indexOf('.') === 0) { return; }
                var str_empty = fs.existsSync(base_folder_P + file + '/meters.txt') ? '' : ' (empty)';
                var folderLabel = file + str_empty;
                if (file.includes('_')) {
                    var partFile = file.split('_');
                    folderLabel = (partFile[1] || '').replace(upperc, function (c) { return c.toUpperCase(); }) + '-' + (partFile[2] || '') + ' ' + partFile[0] + str_empty;
                }
                self.configManager.pushUIConfigParam(uiconf, P('activeFolder'), { value: file, label: folderLabel });
            });
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
    var pos_type = use_SDL2 ? (confData.positionType.value == 0 ? 'center' : 'manual') : 'center';
    if (meterConfig.current['position.type'] !== pos_type) {
        meterConfig.current['position.type'] = pos_type;
        noChanges = false;
    }
    var pos_fit = use_SDL2 && (confData.positionFit === true || confData.positionFit === 'true') ? 'True' : 'False';
    if (String(meterConfig.current['position.fit']) !== pos_fit) {
        meterConfig.current['position.fit'] = pos_fit;
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

    // write the interactive controls setting
    var wantedInteractive = self.interactiveModeOf(confData.interactiveMode && confData.interactiveMode.value);
    if (self.interactiveMode() !== wantedInteractive) {
        meterConfig.current['touch.interactive'] = wantedInteractive;
        noChanges = false;
    }

    // write display port (config.json + live switch)
    // The screen: one of the X displays, or none at all (a player that only serves remote displays).
    var chosenDisplay = String((confData.displayOutput && confData.displayOutput.value) || '0');
    if (chosenDisplay === 'none') {
        if (self.config.get('headless') !== true) {
            self.config.set('headless', true);
            self.logger.info(id + 'display: none of its own, remote displays only');
            noChanges = false;
        }
    } else {
        if (self.config.get('headless') === true) {
            self.config.set('headless', false);
            noChanges = false;
        }
        if (String(self.config.get('displayOutput')) != chosenDisplay) {
            self.config.set('displayOutput', chosenDisplay.replace(/[^0-9]/g, '') || '0');
            self.switch_DisplayPort(parseInt(chosenDisplay, 10));
            noChanges = false;
        }
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

// Themes over the network: Volumio shares its Internal Storage folder, where
// the theme folders live. With smbShareAccess on, the theme trees are made
// writable for the share's guest (directories 777, files 666), now and after
// every install; off, they go back to the player's own permissions.
Glass.prototype.sharingInfo = function () {
    var self = this;
    return {
        shared: self.config.get('smbShareAccess') === true,
        roots: [String(base_folder_P || (DATA_DIR + '/templates/')).replace(/\/$/, ''), String(base_folder_S || (DATA_DIR + '/templates_spectrum/')).replace(/\/$/, '')]
    };
};

Glass.prototype.setSharing = function (on) {
    var self = this;
    var wanted = on === true || on === 'true';
    if ((self.config.get('smbShareAccess') === true) === wanted) { return { changed: false }; }
    self.config.set('smbShareAccess', wanted);
    self.normalizeTemplatePermissions(wanted);
    self.logger.info(id + 'sharing: themes ' + (wanted ? 'writable' : 'read only') + ' over the network share');
    return { changed: true };
};

// After the manager wrote theme folders: keep them writable over the share
// when that is wanted.
Glass.prototype.themesWritten = function () {
    var self = this;
    if (self.config.get('smbShareAccess') === true) { self.normalizeTemplatePermissions(true); }
};

Glass.prototype.normalizeTemplatePermissions = function (enable) {
    var self = this;
    var dirMode = enable ? '777' : '755';
    var fileMode = enable ? '666' : '644';
    self.sharingInfo().roots.forEach(function (dir) {
        if (!fs.existsSync(dir)) { return; }
        // chmod is on the player's sudo list without a password; directories first, then files.
        var cmd = '/usr/bin/sudo -n /bin/chmod -R ' + dirMode + ' ' + JSON.stringify(dir)
            + ' && /usr/bin/find ' + JSON.stringify(dir) + ' -type f -exec /usr/bin/sudo -n /bin/chmod ' + fileMode + ' {} +';
        exec(cmd, function (error) {
            if (error) { self.logger.error(id + 'sharing: ' + dir + ': ' + error); }
        });
    });
};

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
                self.profileTouched();
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

// Themes: the active theme, the tag rules and whether themes survive an
// uninstall. The rest of theme and artwork management is the manager's.
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
        var settings = self.setManagerSettings({
            themeTagRules: (data && typeof data.themeTagRules === 'string') ? data.themeTagRules : '',
            doNotDeleteThemes: !!(data && (data.doNotDeleteThemes === true || data.doNotDeleteThemes === 'true'))
        });
        if (settings.changed) { noChanges = false; }
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
        self.profileTouched();
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
        self.profileTouched();
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

// The configuration as the remotes follow it: its version, the theme on
// show and the meter selection, on the channel to every display connected.
Glass.prototype.pushRemoteConfig = function () {
  var self = this;
  if (!self.channel || !remoteConfigVersion) { return; }
  self.channel.push({ kind: 'config', version: remoteConfigVersion, theme: self.activeTheme(), meter: String((meterConfig && meterConfig.current && meterConfig.current.meter) || '') });
};

Glass.prototype.updateConfigVersion = function () {
  const self = this;
  
  try {
    if (fs.existsSync(MeterConfigFile)) {
      var configContent = fs.readFileSync(MeterConfigFile, 'utf8');
      // The version covers what a remote brings: the meter configuration,
      // and for a display with a face whose the screen is and its look.
      var faceContent = '';
      try { faceContent = JSON.stringify(self.remoteFace()); } catch (e) { /* the configuration alone */ }
      var newHash = crypto.createHash('md5').update(configContent).update(faceContent).digest('hex').substring(0, 8);
      
      if (newHash !== remoteConfigVersion) {
        remoteConfigVersion = newHash;
        self.logger.info(id + 'Config version updated: ' + remoteConfigVersion);
        // The remotes hear of it at once, and the beacon carries it from now.
        self.pushRemoteConfig();
        self.sendBeacon();
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
  // Non-Latin names (Thai, CJK, Cyrillic and so on): ASCII slug would be empty and
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

// The manager's clear: the cached pictures go and the display reloads.
Glass.prototype.clearFanartImages = function () {
  var self = this;
  try {
    self._clearFanartImageCache();
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    return { ok: true };
  } catch (e) {
    self.logger.error(id + 'clearFanartImages: ' + e.message);
    return { error: 'GLASS.CLEAR_FANART_CACHE_FAILED', message: e.message };
  }
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


// A theme folder as the Themes tab lists them: any name that is not hidden
// and stays inside the templates root. A name without an underscore is a
// theme too (1280x400 is one).
Glass.prototype.isValidThemeFolderName = function (folder) {
  return !!folder && typeof folder === 'string' &&
    folder.indexOf('/') === -1 && folder.indexOf('..') === -1 && folder.indexOf('.') !== 0;
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

// Remove a theme from both trees. The last theme stays; removing the
// active one moves the display to another first so the configuration
// never names a folder that is gone.
Glass.prototype.removeTheme = function (folder) {
  var self = this;
  if (!self.isValidThemeFolderName(folder) || !safeFolderName(folder)) {
    return { error: 'GLASS.THEME_REMOVE_INVALID' };
  }
  try {
    self.loadConfigs();
  } catch (e) {
    self.logger.error(id + 'removeTheme: failed to reload config: ' + e.message);
    return { error: 'GLASS.THEME_REMOVE_INVALID' };
  }
  if (!meterConfig || !fs.existsSync(base_folder_P + folder)) {
    return { error: 'GLASS.THEME_REMOVE_INVALID' };
  }

  var allFolders = [];
  try {
    fs.readdirSync(base_folder_P).forEach(function (f) {
      if (f.indexOf('.') !== 0 && fs.statSync(base_folder_P + f).isDirectory()) {
        allFolders.push(f);
      }
    });
  } catch (e) {
    self.logger.error(id + 'removeTheme: enumerate failed: ' + e.message);
  }
  var remaining = allFolders.filter(function (f) { return f !== folder; });
  if (remaining.length === 0) {
    return { error: 'GLASS.THEME_REMOVE_LAST' };
  }

  var wasActive = (meterConfig.current[meterFolderStr] === folder);
  var switchedTo = null;
  if (wasActive) {
    switchedTo = remaining[0];
    self.applyActiveThemeFolder(switchedTo, { allowBuiltin: true });
  }

  self.removeThemeTreeFolder(base_folder_P, folder);
  self.removeThemeTreeFolder(base_folder_S, folder);

  galleryLog(self.logger, 'basic', 'removeTheme removed ' + folder + (wasActive ? ' (was active -> ' + switchedTo + ')' : ''));
  uiNeedsUpdate = true;
  self.updateUIConfig();
  return { ok: true, switchedTo: switchedTo };
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
    return chain.then(function () {
        self.config.set('sideOutputsRetired', true);
    });
};

// Soloist chooses its ALSA device when its daemon starts, from the ALSA
// file as it stands. Its stream is measured on the player's own path,
// `plug:volumio`, where the tap sits; a `plug` put straight on the tap
// (`plug:spotify`, Soloist's metering device of the PeppyMeter days)
// leaves libasound's parameter negotiation with an empty interval and
// aborts the daemon on play, so Glass never asks for that. What can go
// wrong is the moment of the choice: at a backend start the file is
// written in stages, and a device read too early (`softvolume`) sits below
// the tap, so the stream is heard but never measured. Hence the nudge:
// Soloist compares the device it runs with against the file as it is now,
// and restarts its daemon only when they differ.
Glass.prototype.soloistMeteringWanted = function () {
    return false;
};

Glass.prototype.nudgeSoloist = function () {
    var self = this;
    if (self.getPluginStatus('music_service', 'soloist_connect') !== 'STARTED') { return libQ.resolve(); }
    var defer = libQ.defer();
    try {
        var ret = self.commandRouter.executeOnPlugin('music_service', 'soloist_connect', 'setPeppyMetering', false);
        self.logger.info(id + 'Soloist nudged: the player\'s device, measured on the way');
        if (ret && typeof ret.then === 'function') {
            ret.then(function () { defer.resolve(); }, function () { defer.resolve(); });
        } else {
            defer.resolve();
        }
    } catch (e) {
        self.logger.warn(id + 'Soloist: ' + (e && e.message ? e.message : e));
        defer.resolve();
    }
    return defer.promise;
};

// The player rewrites the ALSA file as plugins start; each rewrite after
// Glass's own gets Soloist nudged again, a few seconds after the last.
Glass.prototype.watchAlsaFile = function () {
    var self = this;
    self.unwatchAlsaFile();
    var timer = null;
    self.alsaWatcher = function (curr, prev) {
        if (!curr || !prev || curr.mtimeMs === prev.mtimeMs) { return; }
        if (timer) { clearTimeout(timer); }
        timer = setTimeout(function () {
            timer = null;
            self.nudgeSoloist();
        }, 5000);
    };
    try {
        fs.watchFile(ALSA_FILE, { interval: 2000, persistent: false }, self.alsaWatcher);
    } catch (e) {
        self.logger.warn(id + 'ALSA file watch: ' + (e && e.message ? e.message : e));
    }
};

Glass.prototype.unwatchAlsaFile = function () {
    if (this.alsaWatcher) {
        try { fs.unwatchFile(ALSA_FILE, this.alsaWatcher); } catch (e) {}
        this.alsaWatcher = null;
    }
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

// A backup by name: config.json, the meter and the spectrum configuration
// under a named directory with a manifest. Results carry an error code
// that is a string of the plugin's, so the manager's page can say it.
Glass.prototype.backupCreate = function (name, options) {
    var self = this;
    options = options || {};
    try {
        var backupName = String(name === undefined || name === null ? '' : name).trim();
        if (!backupName) { return { error: 'GLASS.BACKUP_NAME_REQUIRED' }; }
        if (!BackupNameRegex.test(backupName) || backupName.indexOf('..') !== -1) {
            return { error: 'GLASS.BACKUP_NAME_INVALID' };
        }
        if (!fs.existsSync(BackupsPath)) {
            fs.mkdirSync(BackupsPath, { recursive: true });
        }
        var targetDir = BackupsPath + '/' + backupName;
        if (fs.existsSync(targetDir)) { return { error: 'GLASS.BACKUP_NAME_EXISTS' }; }
        try {
            if (typeof fs.statfsSync === 'function') {
                var stats = fs.statfsSync(BackupsPath);
                if (stats.bavail * stats.bsize < BackupMinFreeBytes) {
                    return { error: 'GLASS.BACKUP_DISK_FULL' };
                }
            }
        } catch (e) {
            self.logger.warn(id + 'backupCreate: statfsSync failed, skipping disk check: ' + e.message);
        }
        var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
        var sources = [[configFile, 'config.json'], [MeterConfigFile, 'meter.txt'], [SpectrumConfigFile, 'spectrum.txt']];
        for (var i = 0; i < sources.length; i++) {
            if (!fs.existsSync(sources[i][0])) {
                return { error: 'GLASS.BACKUP_SOURCE_MISSING', message: sources[i][1] };
            }
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
        // Said either way: an upgrade's own are kept to the newest few, a user's are never touched.
        manifest.automatic = options.automatic === true;
        fs.writeFileSync(targetDir + '/' + BackupManifestName, JSON.stringify(manifest, null, 2));
        self.logger.info(id + 'backupCreate: created backup "' + backupName + '"');
        return { ok: true, name: backupName, warn: self.listSettingsBackups().length >= BackupWarnCount ? 'GLASS.BACKUP_COUNT_WARN' : null };
    } catch (e) {
        self.logger.error(id + 'backupCreate: ' + e.message);
        return { error: 'GLASS.BACKUP_CREATE_FAILED', message: e.message };
    }
};

// The backups an upgrade writes on its own accumulate one per upgrade or
// rollback; the newest `keep` stay and the rest go. A backup counts as
// automatic by its manifest; one whose manifest does not say, by the
// `before-<version>` name the upgrades gave theirs before the manifest
// said so (update.automaticBackup). A user's backups are never touched.
Glass.prototype.backupPruneAutomatic = function (keep) {
    var self = this;
    var removed = [];
    try {
        var automatic = self.listSettingsBackups().filter(function (b) {
            try {
                var manifest = JSON.parse(fs.readFileSync(b.path + '/' + BackupManifestName, 'utf8'));
                return automaticBackup(manifest, b.name);
            } catch (e) {
                return false;
            }
        });
        automatic.slice(Math.max(0, keep)).forEach(function (b) {
            var found = self.backupDir(b.name);
            if (found.error) { return; }
            fs.removeSync(found.dir);
            removed.push(b.name);
        });
        if (removed.length) { self.logger.info(id + 'backups: pruned automatic backups ' + removed.join(', ')); }
        return { ok: true, removed: removed };
    } catch (e) {
        self.logger.warn(id + 'backups: prune: ' + e.message);
        return { error: 'GLASS.BACKUP_DELETE_FAILED', message: e.message, removed: removed };
    }
};

// A backup's directory, when its name is one and it exists.
Glass.prototype.backupDir = function (name) {
    var backupName = String(name === undefined || name === null ? '' : name).trim();
    if (!backupName) { return { error: 'GLASS.BACKUP_NOT_SELECTED' }; }
    if (!BackupNameRegex.test(backupName) || backupName.indexOf('..') !== -1 || backupName.indexOf('/') !== -1 || backupName.indexOf('\\') !== -1) {
        return { error: 'GLASS.BACKUP_NAME_INVALID' };
    }
    var dir = BackupsPath + '/' + backupName;
    var resolved = path.resolve(dir);
    var root = path.resolve(BackupsPath);
    if (resolved.indexOf(root + '/') !== 0) { return { error: 'GLASS.BACKUP_NAME_INVALID' }; }
    if (!fs.existsSync(dir)) { return { error: 'GLASS.BACKUP_NOT_FOUND' }; }
    return { name: backupName, dir: dir };
};

// Restore a backup by name. Everything in it is parsed before a live file
// is touched, so a damaged backup changes nothing.
Glass.prototype.backupRestore = function (name) {
    var self = this;
    try {
        var found = self.backupDir(name);
        if (found.error) { return found; }
        var sourceDir = found.dir;
        var manifestPath = sourceDir + '/' + BackupManifestName;
        if (!fs.existsSync(manifestPath)) { return { error: 'GLASS.BACKUP_NOT_FOUND' }; }
        var manifest;
        try {
            manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
        } catch (e) {
            return { error: 'GLASS.BACKUP_MANIFEST_INVALID' };
        }
        if (!manifest || typeof manifest !== 'object' || manifest.schema_version === undefined) {
            return { error: 'GLASS.BACKUP_MANIFEST_INVALID' };
        }
        if (manifest.schema_version > BackupSchemaVersion) {
            return { error: 'GLASS.BACKUP_SCHEMA_UNSUPPORTED' };
        }
        var srcConfigJson = sourceDir + '/config.json';
        var srcPeppyConf = sourceDir + '/' + PeppyConfBackupName;
        var srcSpectrumConf = sourceDir + '/' + SpectrumConfBackupName;
        if (!fs.existsSync(srcConfigJson) || !fs.existsSync(srcPeppyConf) || !fs.existsSync(srcSpectrumConf)) {
            return { error: 'GLASS.BACKUP_FILES_MISSING' };
        }
        try { JSON.parse(fs.readFileSync(srcConfigJson, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_CONFIG_CORRUPT' }; }
        try { ini.parse(fs.readFileSync(srcPeppyConf, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_PEPPYCONF_CORRUPT' }; }
        try { ini.parse(fs.readFileSync(srcSpectrumConf, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_SPECTRUMCONF_CORRUPT' }; }

        var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
        fs.copySync(srcConfigJson, configFile);
        fs.copySync(srcPeppyConf, MeterConfigFile);
        fs.copySync(srcSpectrumConf, SpectrumConfigFile);

        // The caches follow the files, so later saves keep the restored values.
        self.config.loadFile(configFile);
        self.loadConfigs();

        // What the restored config.json decides is applied again: the
        // display port at the next launch and whether the themes stay.
        var dispOut = parseInt(self.config.get('displayOutput'), 10);
        self.switch_DisplayPort(dispOut);
        self.syncPreserveFlag(self.config.get('doNotDeleteThemes') === true);
        self.updateConfigVersion();
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }

        self.logger.info(id + 'backupRestore: restored backup "' + found.name + '"');
        uiNeedsUpdate = true;
        self.updateUIConfig();
        return { ok: true, name: found.name };
    } catch (e) {
        self.logger.error(id + 'backupRestore: ' + e.message);
        return { error: 'GLASS.BACKUP_RESTORE_FAILED', message: e.message };
    }
};

// Take a backup unpacked by the manager into the backups: its manifest
// and files are checked the way a restore checks them, and it lands
// under its own name, or the name asked for, with a number when taken.
Glass.prototype.backupAdopt = function (stagingDir, wantedName) {
    var self = this;
    try {
        var manifestPath = stagingDir + '/' + BackupManifestName;
        if (!fs.existsSync(manifestPath)) { return { error: 'GLASS.BACKUP_MANIFEST_INVALID' }; }
        var manifest;
        try {
            manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
        } catch (e) {
            return { error: 'GLASS.BACKUP_MANIFEST_INVALID' };
        }
        if (!manifest || typeof manifest !== 'object' || manifest.schema_version === undefined) {
            return { error: 'GLASS.BACKUP_MANIFEST_INVALID' };
        }
        if (manifest.schema_version > BackupSchemaVersion) { return { error: 'GLASS.BACKUP_SCHEMA_UNSUPPORTED' }; }
        var files = ['config.json', PeppyConfBackupName, SpectrumConfBackupName];
        for (var i = 0; i < files.length; i++) {
            if (!fs.existsSync(stagingDir + '/' + files[i])) { return { error: 'GLASS.BACKUP_FILES_MISSING' }; }
        }
        try { JSON.parse(fs.readFileSync(stagingDir + '/config.json', 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_CONFIG_CORRUPT' }; }
        try { ini.parse(fs.readFileSync(stagingDir + '/' + PeppyConfBackupName, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_PEPPYCONF_CORRUPT' }; }
        try { ini.parse(fs.readFileSync(stagingDir + '/' + SpectrumConfBackupName, 'utf8')); } catch (e) { return { error: 'GLASS.BACKUP_SPECTRUMCONF_CORRUPT' }; }

        var base = String(wantedName || manifest.name || 'imported').trim().replace(/[^A-Za-z0-9 _.\-]/g, '-').replace(/^[.\-]+/, '').slice(0, 56).trim() || 'imported';
        if (!BackupNameRegex.test(base) || base.indexOf('..') !== -1) { base = 'imported'; }
        if (!fs.existsSync(BackupsPath)) { fs.mkdirSync(BackupsPath, { recursive: true }); }
        var name = base;
        var n = 1;
        while (fs.existsSync(BackupsPath + '/' + name)) { name = base + '-' + (++n); }
        var target = BackupsPath + '/' + name;
        fs.mkdirSync(target);
        files.forEach(function (f) { fs.copySync(stagingDir + '/' + f, target + '/' + f); });
        manifest.name = name;
        manifest.imported = new Date().toISOString();
        fs.writeFileSync(target + '/' + BackupManifestName, JSON.stringify(manifest, null, 2));
        self.logger.info(id + 'backupAdopt: backup "' + name + '" taken in');
        return { ok: true, name: name };
    } catch (e) {
        self.logger.error(id + 'backupAdopt: ' + e.message);
        return { error: 'GLASS.BACKUP_CREATE_FAILED', message: e.message };
    }
};

// Delete a backup by name; only a directory under the backups root goes.
Glass.prototype.backupDelete = function (name) {
    var self = this;
    try {
        var found = self.backupDir(name);
        if (found.error) { return found; }
        fs.removeSync(found.dir);
        self.logger.info(id + 'backupDelete: deleted backup "' + found.name + '"');
        return { ok: true, name: found.name };
    } catch (e) {
        self.logger.error(id + 'backupDelete: ' + e.message);
        return { error: 'GLASS.BACKUP_DELETE_FAILED', message: e.message };
    }
};

// ---- Remote displays -----------------------------------------------------
// A remote display is Glass on another machine. The player gives it the
// tap's frames over UDP (glass-serve, started here), the channel over TCP,
// a beacon on the network so it is found, and its configuration, theme,
// fonts and icons over the manager. See the wiki's Remotes page.

const REMOTE_DEFAULTS = { remotesEnabled: true, remoteFramesPort: 5580, remoteChannelPort: 5581, remoteBeaconPort: 5579 };
const REMOTE_BEACON_EVERY_MS = 5000;
const REMOTE_SERVE_STATUS = '/tmp/glass_serve.json';
const REMOTE_PROTOCOL = 2;

Glass.prototype.remotePorts = function () {
    var self = this;
    var port = function (key) {
        var v = parseInt(self.config.get(key), 10);
        return (v >= 1024 && v <= 65535) ? v : REMOTE_DEFAULTS[key];
    };
    return {
        enabled: self.config.get('remotesEnabled') !== false,
        frames: port('remoteFramesPort'),
        channel: port('remoteChannelPort'),
        beacon: port('remoteBeaconPort'),
        manager: parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT
    };
};

Glass.prototype.startRemotes = function () {
    var self = this;
    var ports = self.remotePorts();
    if (!ports.enabled) { return; }
    self.remotesStopping = false;
    if (self.channel) { self.channel.listenTcp(ports.channel); }
    self.startServe();
    self.beaconSocket = null;
    try {
        var dgram = require('dgram');
        self.beaconSocket = dgram.createSocket({ type: 'udp4', reuseAddr: true });
        self.beaconSocket.on('error', function (err) {
            self.logger.warn(id + 'beacon: ' + (err && err.message ? err.message : err));
        });
        self.beaconSocket.bind(0, function () {
            try { self.beaconSocket.setBroadcast(true); } catch (e) {}
            self.sendBeacon();
        });
    } catch (e) {
        self.logger.warn(id + 'beacon: ' + (e && e.message ? e.message : e));
    }
    if (self.beaconTimer) { clearInterval(self.beaconTimer); }
    self.beaconTimer = setInterval(function () { self.sendBeacon(); }, REMOTE_BEACON_EVERY_MS);
    self.logger.info(id + 'remotes: frames ' + ports.frames + ', channel ' + ports.channel + ', beacon ' + ports.beacon);
};

// Stop serving: the beacon, the daemon (told to leave, made to after two
// seconds) and the channel's TCP listener. Resolves once the daemon has
// left and the listener closed, so a start right after finds the ports free.
Glass.prototype.stopRemotes = function () {
    var self = this;
    self.remotesStopping = true;
    if (self.beaconTimer) { clearInterval(self.beaconTimer); self.beaconTimer = null; }
    if (self.beaconSocket) { try { self.beaconSocket.close(); } catch (e) {} self.beaconSocket = null; }
    if (self.serveRestart) { clearTimeout(self.serveRestart); self.serveRestart = null; }
    var waits = [];
    var child = self.serveChild;
    self.serveChild = null;
    if (child) {
        child.stopping = true;
        waits.push(new Promise(function (resolve) {
            var done = false;
            var finish = function () { if (!done) { done = true; resolve(); } };
            child.once('exit', finish);
            var force = setTimeout(function () { try { child.kill('SIGKILL'); } catch (e) {} }, 2000);
            child.once('exit', function () { clearTimeout(force); });
            setTimeout(finish, 3000);
            try { child.kill('SIGTERM'); } catch (e) { finish(); }
        }));
    }
    if (self.channel) { waits.push(self.channel.unlistenTcp()); }
    self.serveError = null;
    try { if (fs.existsSync(REMOTE_SERVE_STATUS)) { fs.unlinkSync(REMOTE_SERVE_STATUS); } } catch (e) {}
    return Promise.all(waits).then(function () {});
};

// The manager's remote settings: whether this player serves remotes, and the ports.
Glass.prototype.remoteSettings = function () {
    var ports = this.remotePorts();
    return { enabled: ports.enabled, framesPort: ports.frames, channelPort: ports.channel, beaconPort: ports.beacon, managerPort: ports.manager };
};

Glass.prototype.setRemoteSettings = function (data) {
    var self = this;
    var now = self.remoteSettings();
    var enabled = data.enabled === undefined ? now.enabled : (data.enabled === true || data.enabled === 'true');
    var port = function (key, current) {
        if (data[key] === undefined) { return current; }
        var v = parseInt(data[key], 10);
        return (v >= 1024 && v <= 65535) ? v : NaN;
    };
    var frames = port('framesPort', now.framesPort);
    var channel = port('channelPort', now.channelPort);
    var beacon = port('beaconPort', now.beaconPort);
    if ([frames, channel, beacon].some(isNaN)) { return { error: 'GLASS.MANAGER_RM_PORTS_INVALID' }; }
    var all = [frames, channel, beacon, now.managerPort];
    if (new Set(all).size !== all.length) { return { error: 'GLASS.MANAGER_RM_PORTS_CLASH' }; }
    var changed = enabled !== now.enabled || frames !== now.framesPort || channel !== now.channelPort || beacon !== now.beaconPort;
    self.config.set('remotesEnabled', enabled);
    self.config.set('remoteFramesPort', frames);
    self.config.set('remoteChannelPort', channel);
    self.config.set('remoteBeaconPort', beacon);
    if (!changed) { return Promise.resolve({ ok: true, changed: false }); }
    self.logger.info(id + 'remotes: settings ' + (enabled ? 'on' : 'off') + ' ' + frames + '/' + channel + '/' + beacon);
    return self.stopRemotes().then(function () {
        if (enabled) { self.startRemotes(); } else { self.startServe(); }
        return { ok: true, changed: true };
    });
};

// The frames daemon, restarted a few seconds after it leaves. It serves
// the browser pages on its socket always; the port only while remote
// displays are served.
Glass.prototype.startServe = function () {
    var self = this;
    var ports = self.remotePorts();
    self.remotesStopping = false;
    var arch = self.volumioArch();
    var bin = PluginPath + '/bin/' + arch + '/glass-serve';
    if (!fs.existsSync(bin)) {
        self.logger.error(id + 'remotes: no glass-serve for ' + arch);
        return;
    }
    var child;
    try {
        var log = self.logSettings();
        var args = ['--rate', '60', '--status', REMOTE_SERVE_STATUS, '--face', faceSocketPath].concat(ports.enabled ? ['--port', String(ports.frames)] : ['--local']);
        child = spawn(bin, args, { uid: 1000, gid: 1000, stdio: ['ignore', 'pipe', 'pipe'], env: Object.assign({}, process.env, { GLASS_LOG: log.level, GLASS_LOG_TARGETS: log.targets.join(',') }) });
    } catch (e) {
        self.logger.error(id + 'remotes: glass-serve: ' + (e && e.message ? e.message : e));
        return;
    }
    self.serveChild = child;
    self.serveError = null;
    var startedAt = Date.now();
    var say = function (chunk) {
        String(chunk).split('\n').forEach(function (line) { if (line.trim()) { self.logger.info(id + line.trim()); } });
    };
    child.stdout.on('data', say);
    child.stderr.on('data', function (chunk) {
        say(chunk);
        var last = String(chunk).trim().split('\n').pop();
        if (last) { self.serveError = last.replace(/^glass-serve:\s*/, ''); }
    });
    child.on('exit', function (code, signal) {
        if (self.serveChild === child) { self.serveChild = null; }
        // A daemon told to leave is not one that died.
        if (child.stopping || self.remotesStopping) { return; }
        // A daemon that ran a while starts again at once; one that leaves at
        // once (its port taken, say) is tried again later, later each time.
        if (Date.now() - startedAt > 30000) { self.serveBackoff = 0; }
        self.serveBackoff = Math.min((self.serveBackoff || 2500) * 2, 60000);
        self.logger.warn(id + 'remotes: glass-serve left (' + (signal || code) + (self.serveError ? ', ' + self.serveError : '') + '), starting it again in ' + Math.round(self.serveBackoff / 1000) + ' s');
        self.serveRestart = setTimeout(function () { self.serveRestart = null; if (!self.remotesStopping) { self.startServe(); } }, self.serveBackoff);
    });
};

// What the beacon says about this player.
Glass.prototype.beacon = function () {
    var self = this;
    var ports = self.remotePorts();
    var name = '';
    try { name = String(self.commandRouter.sharedVars.get('system.name') || ''); } catch (e) {}
    return {
        glass: 'player',
        protocol: REMOTE_PROTOCOL,
        name: name || os.hostname(),
        host: self.managerHost(),
        frames_port: ports.frames,
        channel_port: ports.channel,
        manager_port: ports.manager,
        player_port: 3000,
        release: pluginVersion,
        config: remoteConfigVersion,
        theme: self.activeTheme(),
        meter: String((meterConfig && meterConfig.current && meterConfig.current.meter) || '')
    };
};

// Every interface's broadcast address, and the whole-network one.
function broadcastAddresses() {
    var out = ['255.255.255.255'];
    var ifaces = os.networkInterfaces();
    Object.keys(ifaces).forEach(function (name) {
        (ifaces[name] || []).forEach(function (a) {
            if (a.family !== 'IPv4' || a.internal || !a.netmask) { return; }
            var ip = a.address.split('.').map(Number);
            var mask = a.netmask.split('.').map(Number);
            if (ip.length !== 4 || mask.length !== 4) { return; }
            var b = ip.map(function (o, i) { return (o | (~mask[i] & 255)) & 255; }).join('.');
            if (out.indexOf(b) === -1) { out.push(b); }
        });
    });
    return out;
}

Glass.prototype.sendBeacon = function () {
    var self = this;
    if (!self.beaconSocket) { return; }
    var ports = self.remotePorts();
    var message = Buffer.from(JSON.stringify(self.beacon()));
    broadcastAddresses().forEach(function (address) {
        try { self.beaconSocket.send(message, 0, message.length, ports.beacon, address); } catch (e) {}
    });
};

// What the manager shows about the remotes: the daemon's subscribers and
// the channel's remote connections.
Glass.prototype.remoteInfo = function () {
    var self = this;
    var serve = null;
    try { serve = JSON.parse(fs.readFileSync(REMOTE_SERVE_STATUS, 'utf8')); } catch (e) {}
    return {
        ports: self.remotePorts(),
        beacon: self.beacon(),
        serve: serve,
        serving: !!self.serveChild,
        errors: {
            serve: self.serveChild ? null : (self.serveError || null),
            channel: self.channel ? (self.channel.tcpError || null) : null
        },
        headless: self.config.get('headless') === true,
        remotes: self.channel ? self.channel.remotes() : []
    };
};

// A file's SHA-256, remembered by its size and time so the fonts are not read again.
var hashCache = {};
function fileDigest(file) {
    try {
        var stat = fs.statSync(file);
        var key = file + ':' + stat.size + ':' + stat.mtimeMs;
        if (hashCache[file] && hashCache[file].key === key) { return { sha256: hashCache[file].sha256, bytes: stat.size }; }
        var sha256 = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
        hashCache[file] = { key: key, sha256: sha256 };
        return { sha256: sha256, bytes: stat.size };
    } catch (e) {
        return null;
    }
}

// Volumio's own format icons, beside its web application: the display
// looks there after the theme's and the plugin's.
var StockIcons = '/volumio/http/www3/app/assets-common/format-icons';

function isFontFile(name) { return /\.(ttf|otf)$/i.test(name); }
function isIconFile(name) { return /\.(svg|png)$/i.test(name); }

// The directory the meter configuration's font.path names, when it is one.
function fontPathDir() {
    var v = meterConfig && meterConfig.current ? String(meterConfig.current['font.path'] || '').trim() : '';
    v = v.replace(/\/+$/, '');
    try { return v && fs.statSync(v).isDirectory() ? v : null; } catch (e) { return null; }
}

function filesOf(dir, filter) {
    var out = [];
    try {
        fs.readdirSync(dir).sort().forEach(function (name) {
            if (name.indexOf('.') === 0 || (filter && !filter(name))) { return; }
            var d = fileDigest(dir + '/' + name);
            if (d) { out.push({ name: name, sha256: d.sha256, bytes: d.bytes }); }
        });
    } catch (e) {}
    return out;
}

// The configuration and assets a remote brings into its own home.
Glass.prototype.remoteConfig = function () {
    var self = this;
    self.loadConfigs();
    var meterText = '';
    var spectrumText = '';
    try { meterText = fs.readFileSync(MeterConfigFile, 'utf8'); } catch (e) {}
    try { spectrumText = fs.readFileSync(SpectrumConfigFile, 'utf8'); } catch (e) {}
    // The plugin's own icons first, then Volumio's, as the display looks.
    var icons = filesOf(PluginPath + '/format-icons', isIconFile);
    filesOf(StockIcons, isIconFile).forEach(function (icon) {
        if (!icons.some(function (own) { return own.name === icon.name; })) { icons.push(icon); }
    });
    var fontDir = fontPathDir();
    return {
        version: remoteConfigVersion,
        release: pluginVersion,
        theme: self.activeTheme(),
        meter: String((meterConfig && meterConfig.current && meterConfig.current.meter) || ''),
        files: { meter: meterText, spectrum: spectrumText },
        face: self.remoteFace(),
        assets: {
            fonts: filesOf(PluginPath + '/fonts', isFontFile),
            icons: icons,
            webfonts: fontDir ? filesOf(fontDir, isFontFile) : [],
            custom: filesOf(CustomFontsPath, isFontFile)
        }
    };
};

// A theme's files with their checksums, the spectrum twin included.
Glass.prototype.themeFiles = function (folder) {
    var self = this;
    self.loadConfigs();
    if (!safeFolderName(folder)) { return null; }
    var walk = function (root) {
        var files = [];
        var visit = function (dir, prefix) {
            var names = [];
            try { names = fs.readdirSync(dir).sort(); } catch (e) { return; }
            names.forEach(function (name) {
                if (name.indexOf('.') === 0) { return; }
                var full = dir + '/' + name;
                var stat;
                try { stat = fs.lstatSync(full); } catch (e) { return; }
                if (stat.isDirectory()) { visit(full, prefix + name + '/'); }
                else if (stat.isFile()) {
                    var d = fileDigest(full);
                    if (d) { files.push({ path: prefix + name, sha256: d.sha256, bytes: d.bytes }); }
                }
            });
        };
        visit(root, '');
        return files;
    };
    var meterDir = base_folder_P + folder;
    if (!fs.existsSync(meterDir + '/meters.txt')) { return null; }
    var out = { folder: folder, files: walk(meterDir), spectrum: null };
    var spectrumDir = (base_folder_S || '') + folder;
    if (base_folder_S && fs.existsSync(spectrumDir + '/spectrum.txt')) {
        out.spectrum = { folder: folder, files: walk(spectrumDir) };
    }
    return out;
};

// One file of a theme, from either tree, only from inside it.
Glass.prototype.themeFilePath = function (folder, tree, relative) {
    var self = this;
    self.loadConfigs();
    if (!safeFolderName(folder)) { return null; }
    var base = tree === 'templates_spectrum' ? base_folder_S : (tree === 'templates' ? base_folder_P : null);
    if (!base) { return null; }
    var root = path.resolve(base + folder);
    var target = path.resolve(root, String(relative || ''));
    if (target.indexOf(root + path.sep) !== 0) { return null; }
    try { if (!fs.statSync(target).isFile()) { return null; } } catch (e) { return null; }
    return target;
};

// A font, an icon, a web font or an uploaded font by name, from the
// directories the display looks in, in their order: the plugin's icons
// before Volumio's, the web fonts from the directory the meter
// configuration's font.path names, the uploaded fonts from their own.
Glass.prototype.assetPath = function (kind, name) {
    if (typeof name !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,127}$/.test(name)) { return null; }
    var dirs = [];
    if (kind === 'font' && isFontFile(name)) { dirs = [PluginPath + '/fonts']; }
    else if (kind === 'custom' && isFontFile(name)) { dirs = [CustomFontsPath]; }
    else if (kind === 'icon' && isIconFile(name)) { dirs = [PluginPath + '/format-icons', StockIcons]; }
    else if (kind === 'webfont' && isFontFile(name)) {
        if (!meterConfig) { this.loadConfigs(); }
        var fontDir = fontPathDir();
        if (fontDir) { dirs = [fontDir]; }
    }
    for (var i = 0; i < dirs.length; i++) {
        var file = dirs[i] + '/' + name;
        try { if (fs.statSync(file).isFile()) { return file; } } catch (e) {}
    }
    return null;
};

// ---- The manager's view of the plugin --------------------------------
// The manager (manager/server.js) reaches the plugin through these. They
// return plain results and show no toasts: the manager's page speaks for
// itself, and an error is the code of one of the plugin's strings.

// Start the manager on the configured port, or on `port`. Resolves once
// it listens; rejects, with no manager kept, when the port is taken.
Glass.prototype.startManager = function (port) {
    var self = this;
    if (self.manager) { return Promise.resolve(self.manager); }
    var manager = new Manager(self);
    self.manager = manager;
    var wanted = parseInt(port, 10) || parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT;
    return manager.start(wanted).then(function () {
        return manager;
    }, function (e) {
        self.logger.error(id + 'manager: port ' + wanted + ': ' + (e && e.message ? e.message : e));
        if (self.manager === manager) { self.manager = null; }
        throw e;
    });
};

Glass.prototype.stopManager = function () {
    var self = this;
    var manager = self.manager;
    self.manager = null;
    if (!manager) { return Promise.resolve(); }
    return manager.stop().catch(function (e) {
        self.logger.warn(id + 'manager stop: ' + (e && e.message ? e.message : e));
    });
};

// The manager's port and address from the settings page. A new port is
// taken at once; when it is in use the old one stays and the page says so.
Glass.prototype.saveManagerConf = function (data) {
    var self = this;
    var defer = libQ.defer();
    var pluginName = self.commandRouter.getI18nString('GLASS.PLUGIN_NAME');
    var port = parseInt(data && data.managerPort, 10);
    if (isNaN(port) || port < 1024 || port > 65535) {
        self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.MANAGER_PORT_INVALID'));
        self.updateUIConfig();
        defer.resolve();
        return defer.promise;
    }
    var host = String((data && data.managerHost) || '').trim().toLowerCase().replace(/^[a-z]+:\/\//, '').replace(/[/].*$/, '');
    if (host && !/^[a-z0-9]([a-z0-9.-]*[a-z0-9])?$/.test(host)) {
        self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.MANAGER_HOST_INVALID'));
        self.updateUIConfig();
        defer.resolve();
        return defer.promise;
    }
    var changed = false;
    if (host !== String(self.config.get('managerHost') || '')) {
        self.config.set('managerHost', host);
        changed = true;
    }
    var oldPort = parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT;
    if (port === oldPort) {
        self.commandRouter.pushToastMessage(changed ? 'success' : 'info', pluginName, self.commandRouter.getI18nString(changed ? 'COMMON.SETTINGS_SAVED_SUCCESSFULLY' : 'GLASS.NO_CHANGES'));
        self.updateUIConfig();
        defer.resolve();
        return defer.promise;
    }
    self.stopManager().then(function () {
        return self.startManager(port);
    }).then(function () {
        self.config.set('managerPort', port);
        self.commandRouter.pushToastMessage('success', pluginName, self.commandRouter.getI18nString('GLASS.MANAGER_MOVED') + ' ' + port);
        self.updateUIConfig();
        defer.resolve();
    }, function () {
        self.commandRouter.pushToastMessage('error', pluginName, self.commandRouter.getI18nString('GLASS.MANAGER_PORT_IN_USE') + ' ' + oldPort);
        self.startManager(oldPort).catch(function () {});
        self.updateUIConfig();
        defer.resolve();
    });
    return defer.promise;
};

// The player's host name as mDNS announces it.
Glass.prototype.managerDefaultHost = function () {
    var host = '';
    try { host = os.hostname(); } catch (e) {}
    host = String(host || '').trim().toLowerCase();
    if (!host) {
        try { host = String(this.commandRouter.sharedVars.get('system.name') || '').trim().toLowerCase().replace(/[^a-z0-9-]+/g, '-'); } catch (e) {}
    }
    return (host || 'volumio') + '.local';
};

// Where a browser on the network reaches the manager: the address set in
// the settings when there is one, else the mDNS name.
Glass.prototype.managerHost = function () {
    var override = String(this.config.get('managerHost') || '').trim();
    return override || this.managerDefaultHost();
};

Glass.prototype.managerUrl = function () {
    var self = this;
    var port = parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT;
    return 'http://' + self.managerHost() + ':' + port + '/';
};

Glass.prototype.managerLanguage = function () {
    try { return String(this.commandRouter.sharedVars.get('language_code') || 'en'); } catch (e) { return 'en'; }
};

// The plugin's strings in a language, with English behind them.
Glass.prototype.managerStrings = function (lang) {
    var read = function (code) {
        try {
            return JSON.parse(fs.readFileSync(__dirname + '/i18n/strings_' + code + '.json', 'utf8')).GLASS || null;
        } catch (e) {
            return null;
        }
    };
    var base = read('en') || {};
    var wanted = (lang && /^[a-z]{2,5}$/i.test(lang) && lang !== 'en') ? read(lang) : null;
    return Object.assign({}, base, wanted || {});
};

Glass.prototype.managerPaths = function () {
    var self = this;
    self.loadConfigs();
    return {
        pluginPath: PluginPath,
        dataDir: DATA_DIR,
        evoDir: EVO_DIR,
        meterBase: String(base_folder_P || (DATA_DIR + '/templates/')).replace(/\/$/, ''),
        spectrumBase: String(base_folder_S || (DATA_DIR + '/templates_spectrum/')).replace(/\/$/, ''),
        launcher: LaunchScript,
        version: pluginVersion
    };
};

Glass.prototype.activeTheme = function () {
    return (meterConfig && meterConfig.current) ? String(meterConfig.current[meterFolderStr] || '') : '';
};

// The sections of a meters or spectrum file, or none.
function themeSections(file) {
    try {
        return configSections(fs.readFileSync(file, 'utf8'));
    } catch (e) {
        return [];
    }
}

// Every folder under the meter templates, with what it holds and whether
// a spectrum twin stands beside it.
Glass.prototype.themeList = function () {
    var self = this;
    self.loadConfigs();
    var active = self.activeTheme();
    var list = [];
    var entries = [];
    try { entries = fs.readdirSync(base_folder_P); } catch (e) { return list; }
    entries.sort().forEach(function (folder) {
        if (folder.indexOf('.') === 0) { return; }
        var dir = base_folder_P + folder;
        var stat;
        try { stat = fs.statSync(dir); } catch (e) { return; }
        if (!stat.isDirectory()) { return; }
        var hasMeters = fs.existsSync(dir + '/meters.txt');
        var spectrumDir = (base_folder_S || '') + folder;
        var hasSpectrum = !!base_folder_S && fs.existsSync(spectrumDir + '/spectrum.txt');
        var bytes = 0;
        var files = 0;
        try {
            fs.readdirSync(dir).forEach(function (f) {
                try {
                    var s = fs.statSync(dir + '/' + f);
                    if (s.isFile()) { bytes += s.size; files++; }
                } catch (e) {}
            });
        } catch (e) {}
        list.push({
            folder: folder,
            meters: hasMeters ? themeSections(dir + '/meters.txt') : [],
            spectra: hasSpectrum ? themeSections(spectrumDir + '/spectrum.txt') : [],
            spectrum: hasSpectrum,
            empty: !hasMeters,
            bytes: bytes,
            files: files,
            mtime: stat.mtimeMs,
            active: folder === active,
            builtin: folder.indexOf('_') === -1
        });
    });
    return list;
};

Glass.prototype.activateTheme = function (folder) {
    var self = this;
    if (!safeFolderName(folder)) { return { changed: false, error: 'invalid' }; }
    self.loadConfigs();
    if (!meterConfig) { return { changed: false, error: 'no_config' }; }
    return self.applyActiveThemeFolder(folder, { allowBuiltin: true });
};

// The meter rotation of the active theme: one meter, a list, or all of
// them, changing on a timer or with the title.
Glass.prototype.meterSelection = function () {
    var self = this;
    self.loadConfigs();
    var current = (meterConfig && meterConfig.current) || {};
    var theme = self.activeTheme();
    var available = theme ? themeSections(base_folder_P + theme + '/meters.txt') : [];
    var raw = String(current.meter || 'random').trim();
    var mode = raw === 'random' ? 'random' : (raw.indexOf(',') !== -1 ? 'list' : 'single');
    var names = mode === 'random' ? [] : raw.split(',').map(function (s) { return s.trim(); }).filter(Boolean);
    return {
        theme: theme,
        mode: mode,
        names: names,
        available: available,
        onTitle: String(current['random.change.title'] || 'False').toLowerCase() === 'true',
        interval: parseInt(current['random.meter.interval'], 10) || 60
    };
};

Glass.prototype.setMeterSelection = function (data) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var now = self.meterSelection();
    var mode = String(data.mode || now.mode);
    if (['random', 'list', 'single'].indexOf(mode) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var names = Array.isArray(data.names) ? data.names.map(function (s) { return String(s).trim(); }).filter(Boolean) : now.names;
    if (mode !== 'random') {
        if (names.length === 0 || (mode === 'single' && names.length !== 1)) { return { error: 'GLASS.MANAGER_METER_NAMES' }; }
        var unknown = names.filter(function (n) { return now.available.indexOf(n) === -1; });
        if (unknown.length) { return { error: 'GLASS.MANAGER_METER_UNKNOWN', message: unknown.join(', ') }; }
    }
    var onTitle = data.onTitle === undefined ? now.onTitle : (data.onTitle === true || data.onTitle === 'true');
    var interval = data.interval === undefined ? now.interval : parseInt(data.interval, 10);
    if (isNaN(interval)) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    interval = Math.min(1000, Math.max(15, interval));
    var meter = mode === 'random' ? 'random' : names.join(',');
    var changed = false;
    if (String(meterConfig.current.meter) !== meter) { meterConfig.current.meter = meter; changed = true; }
    var title = onTitle ? 'True' : 'False';
    if (String(meterConfig.current['random.change.title']) !== title) { meterConfig.current['random.change.title'] = title; changed = true; }
    if (parseInt(meterConfig.current['random.meter.interval'], 10) !== interval) { meterConfig.current['random.meter.interval'] = interval; changed = true; }
    self.config.set('randomSelection', mode === 'list' ? meter : '');
    if (changed) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        uiNeedsUpdate = true;
        self.updateUIConfig();
    }
    return { ok: true, changed: changed };
};

// What the old plugin's folder still holds, and whether that plugin is
// still installed, for the manager's System tab.
Glass.prototype.legacyThemes = function () {
    var info = legacyThemes.legacyThemes(LEGACY_DATA);
    info.installed = fs.existsSync('/data/plugins/user_interface/' + LEGACY_PLUGIN);
    return info;
};

// Remove the old plugin's theme trees, at the listener's request.
Glass.prototype.wipeLegacyThemes = function () {
    var self = this;
    var removed = legacyThemes.wipeLegacyThemes(LEGACY_DATA);
    self.logger.info(id + 'themes: ' + LEGACY_PLUGIN + "'s " + (removed.length ? removed.join(' and ') : 'nothing') + ' removed at the request of the manager');
    return { ok: true, removed: removed };
};

// Where the theme goes on the screen: fitted to it, scaled with its shape
// kept, or pixel for pixel; centred, or with its top left at a position.
Glass.prototype.displayPlacement = function () {
    var self = this;
    self.loadConfigs();
    var cur = (meterConfig && meterConfig.current) || {};
    var type = String(cur['position.type'] || 'center').toLowerCase();
    return {
        fit: String(cur['position.fit']).toLowerCase() === 'true' || type === 'fit',
        position: type === 'center' || type === 'fit' ? 'center' : 'manual',
        x: parseInt(cur['position.x'], 10) || 0,
        y: parseInt(cur['position.y'], 10) || 0
    };
};

// The same, set: the meter configuration is written, the displays hear of
// the change and the player's display starts again with it.
Glass.prototype.setDisplayPlacement = function (data) {
    var self = this;
    self.loadConfigs();
    if (!meterConfig || !fs.existsSync(MeterConfigFile)) { return { error: 'GLASS.NO_PEPPYCONFIG' }; }
    var now = self.displayPlacement();
    var fit = data.fit === undefined ? now.fit : (data.fit === true || data.fit === 'true');
    var position = data.position === undefined ? now.position : String(data.position);
    if (['center', 'manual'].indexOf(position) === -1) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var x = data.x === undefined ? now.x : parseInt(data.x, 10);
    var y = data.y === undefined ? now.y : parseInt(data.y, 10);
    // Either way from the screen's corner: a picture larger than the screen,
    // or one with a margin drawn into it, is placed with a negative start.
    if (isNaN(x) || isNaN(y) || Math.abs(x) > 7680 || Math.abs(y) > 4320) { return { error: 'GLASS.MANAGER_BAD_REQUEST' }; }
    var wanted = { 'position.fit': fit ? 'True' : 'False', 'position.type': position, 'position.x': String(x), 'position.y': String(y) };
    var changed = false;
    Object.keys(wanted).forEach(function (k) {
        if (String(meterConfig.current[k]) !== wanted[k]) { meterConfig.current[k] = wanted[k]; changed = true; }
    });
    if (changed) {
        fs.writeFileSync(MeterConfigFile, ini.stringify(meterConfig, { whitespace: true }));
        try { self.updateConfigVersion(); } catch (e) {}
        if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
        uiNeedsUpdate = true;
        self.updateUIConfig();
    }
    return { ok: true, changed: changed };
};

// The artist fanart slideshow's settings.
Glass.prototype.artworkSettings = function () {
    var self = this;
    var keyMode = self.config.get('fanartKeyMode') || 'personal';
    var order = self.config.get('fanartOrder') || 'sequential';
    var transition = self.config.get('fanartTransition') || 'none';
    return {
        enabled: self.config.get('fanartEnabled') === true,
        keyMode: keyMode === 'project' ? 'project' : 'personal',
        personalKey: String(self.config.get('fanart_personal_key') || ''),
        interval: parseInt(self.config.get('fanartInterval'), 10) || 0,
        order: order === 'random' ? 'random' : 'sequential',
        transition: ['none', 'fade', 'merge'].indexOf(transition) === -1 ? 'none' : transition,
        transitionMs: parseInt(self.config.get('fanartTransitionMs'), 10) || 600,
        unlimited: self.config.get('fanartUnlimitedImages') === true,
        maxImages: FANART_MAX_IMAGES
    };
};

Glass.prototype.setArtworkSettings = function (data) {
    var self = this;
    var now = self.artworkSettings();
    var pickBool = function (v, fallback) { return v === undefined ? fallback : (v === true || v === 'true'); };
    var enabled = pickBool(data.enabled, now.enabled);
    var keyMode = data.keyMode === undefined ? now.keyMode : (data.keyMode === 'project' ? 'project' : 'personal');
    var key = data.personalKey === undefined ? now.personalKey : String(data.personalKey).trim();
    var interval = data.interval === undefined ? now.interval : parseInt(data.interval, 10);
    if (isNaN(interval) || interval < 0) { interval = 0; }
    if (interval > 3600) { interval = 3600; }
    var transition = data.transition === undefined ? now.transition : String(data.transition);
    if (['none', 'fade', 'merge'].indexOf(transition) === -1) { transition = 'none'; }
    var transitionMs = data.transitionMs === undefined ? now.transitionMs : parseInt(data.transitionMs, 10);
    if (isNaN(transitionMs) || transitionMs < 50) { transitionMs = 600; }
    if (transitionMs > 3000) { transitionMs = 3000; }
    var order = data.order === undefined ? now.order : (data.order === 'random' ? 'random' : 'sequential');
    var unlimited = pickBool(data.unlimited, now.unlimited);
    var wanted = { fanartEnabled: enabled, fanartKeyMode: keyMode, fanart_personal_key: key, fanartInterval: interval, fanartTransition: transition, fanartTransitionMs: transitionMs, fanartOrder: order, fanartUnlimitedImages: unlimited };
    var changed = false;
    Object.keys(wanted).forEach(function (k) {
        if (self.config.get(k) !== wanted[k]) {
            self.config.set(k, wanted[k]);
            changed = true;
        }
    });
    if (unlimited && !now.unlimited) {
        try { self._clearFanartImageCache(); } catch (e) {}
    }
    if (changed && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    return { ok: true, changed: changed };
};

// The settings the manager and the settings page share.
Glass.prototype.managerSettings = function () {
    var self = this;
    return {
        themeTagRules: String(self.config.get('themeTagRules') || ''),
        doNotDeleteThemes: self.config.get('doNotDeleteThemes') === true,
        managerPort: parseInt(self.config.get('managerPort'), 10) || MANAGER_DEFAULT_PORT,
        managerHost: String(self.config.get('managerHost') || '')
    };
};

Glass.prototype.setManagerSettings = function (data) {
    var self = this;
    var changed = false;
    if (data.themeTagRules !== undefined) {
        var rules = String(data.themeTagRules || '').slice(0, 800);
        if ((self.config.get('themeTagRules') || '') !== rules) {
            self.config.set('themeTagRules', rules);
            changed = true;
            try { self.applyThemeTag(self.lastState); } catch (e) {}
        }
    }
    if (data.doNotDeleteThemes !== undefined) {
        var preserve = data.doNotDeleteThemes === true || data.doNotDeleteThemes === 'true';
        if ((self.config.get('doNotDeleteThemes') === true) !== preserve) {
            self.config.set('doNotDeleteThemes', preserve);
            changed = true;
        }
        self.syncPreserveFlag(preserve);
    }
    return { ok: true, changed: changed };
};

// After the manager put folders in place: the lists are read again, the
// settings page's theme list follows, and a display showing a theme that
// was replaced starts over with the new files.
Glass.prototype.afterThemesChanged = function (folders) {
    var self = this;
    self.loadConfigs();
    var active = self.activeTheme();
    var replacedActive = (folders || []).some(function (f) { return f.install === 'templates' && f.folder === active; });
    if (replacedActive && fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    try { self.updateConfigVersion(); } catch (e) {}
    uiNeedsUpdate = true;
    self.updateUIConfig();
};

Glass.prototype.backupList = function () {
    return this.listSettingsBackups().map(function (b) {
        return { name: b.name, created: b.createdMs ? new Date(b.createdMs).toISOString() : null, pluginVersion: b.pluginVersion };
    });
};

// Replace this plugin with a zip staged under /tmp/plugins, through the
// player's own plugin manager: it stops this plugin, removes its
// directory, unpacks the zip, runs the install script and enables the
// plugin again. The settings in /data/configuration stay. The code that
// then runs is still this module until the backend restarts.
// ---- Back to the stable release: the settings -----------------------------

// What decides which questions the way back asks: whether glass-evo holds
// the screen, whether it is here, whether this player takes test releases.
Glass.prototype.stableFacts = function () {
    var self = this;
    var owner = { owner: 'kiosk', evo: null };
    try { owner = self.screenOwnerState(); } catch (e) { /* as the kiosk's */ }
    return { holds: owner.owner === 'glass-evo', evo: !!(owner.evo && owner.evo.installed), tests: self.testReleases() };
};

// The settings as they stand, for the plan: the plugin's configuration by
// key, the meter and the spectrum configuration by section.
Glass.prototype.stableCurrent = function () {
    var self = this;
    var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
    var read = function (file, parse) { try { return parse(fs.readFileSync(file, 'utf8')); } catch (e) { return {}; } };
    return { config: read(configFile, JSON.parse), meter: read(MeterConfigFile, ini.parse), spectrum: read(SpectrumConfigFile, ini.parse) };
};

// The settings a release comes with, from the texts of its three files
// (config.json and the two templates under config/); a text left out is
// read from the plugin installed here.
Glass.prototype.stableDefaults = function (texts) {
    var given = texts || {};
    var text = function (name, file) { return typeof given[name] === 'string' ? given[name] : fs.readFileSync(PluginPath + '/' + file, 'utf8'); };
    return {
        config: JSON.parse(text('config', 'config.json')),
        meter: ini.parse(text('meter', 'config/meter.txt.tmpl')),
        spectrum: ini.parse(text('spectrum', 'config/spectrum.txt.tmpl'))
    };
};

// A plan's settings written in place of the ones that stand, and read
// again so that later saves build on them; the display starts again with
// them. What the configuration decides beyond its files is applied as
// after a restore: the display's output and whether the themes stay.
Glass.prototype.stableWrite = function (planned) {
    var self = this;
    var configFile = self.commandRouter.pluginManager.getConfigurationFile(self.context, 'config.json');
    var write = function (file, text) { fs.writeFileSync(file + '.new', text); fs.renameSync(file + '.new', file); };
    write(configFile, JSON.stringify(planned.config, null, 4));
    write(MeterConfigFile, ini.stringify(planned.meter, { whitespace: true }));
    write(SpectrumConfigFile, ini.stringify(planned.spectrum, { whitespace: true }));
    self.config.loadFile(configFile);
    self.loadConfigs();
    try { self.switch_DisplayPort(parseInt(self.config.get('displayOutput'), 10)); } catch (e) { self.logger.warn(id + 'stable: display output: ' + e.message); }
    try { self.syncPreserveFlag(self.config.get('doNotDeleteThemes') === true); } catch (e) { self.logger.warn(id + 'stable: preserve flag: ' + e.message); }
    try { self.noteFrostSuits(); } catch (e) { /* noted at the next start */ }
    try { self.updateConfigVersion(); } catch (e) { /* at the next change */ }
    if (fs.existsSync(runFlag)) { fs.removeSync(runFlag); }
    uiNeedsUpdate = true;
    try { self.updateUIConfig(); } catch (e) { /* the settings page reads them when opened */ }
};

Glass.prototype.updateApply = function (stagedName) {
    var self = this;
    var name = String(stagedName || '');
    if (!/^[A-Za-z0-9._-]+\.zip$/.test(name)) { return Promise.reject(new Error('bad zip name')); }
    self.logger.info(id + 'upgrade: handing ' + name + ' to the plugin manager');
    return new Promise(function (resolve, reject) {
        self.commandRouter.updatePlugin({
            url: 'http://127.0.0.1:3000/plugin-serve/' + name,
            category: 'user_interface',
            name: 'glass'
        }).then(function () {
            // The plugin manager enables the plugin again through its own
            // configuration, which it saves a moment later; the restart must
            // not land before that, or the plugin comes back installed and off.
            self.ensureEnabledInRegistry().then(resolve, resolve);
        }, function (e) { reject(e instanceof Error ? e : new Error(String(e))); });
    });
};

Glass.prototype.ensureEnabledInRegistry = function () {
    var self = this;
    var file = '/data/configuration/plugins.json';
    var pm = self.commandRouter.pluginManager;
    var reads = function () {
        try {
            var p = JSON.parse(fs.readFileSync(file, 'utf8'));
            var g = p.user_interface && p.user_interface.glass;
            return !!(g && g.enabled && g.enabled.value === true);
        } catch (e) { return false; }
    };
    return new Promise(function (resolve) {
        var tries = 0;
        var look = function () {
            if (reads()) { self.logger.info(id + 'upgrade: the registry shows Glass enabled'); return resolve(); }
            if (tries === 2 && pm && pm.config && typeof pm.config.set === 'function') {
                try {
                    pm.config.set('user_interface.glass.enabled', true);
                    pm.config.set('user_interface.glass.status', 'STARTED');
                    self.logger.warn(id + 'upgrade: the registry did not show Glass enabled; set it');
                } catch (e) { self.logger.warn(id + 'upgrade: registry: ' + (e && e.message ? e.message : e)); }
            }
            if (++tries > 12) { self.logger.warn(id + 'upgrade: the registry never showed Glass enabled; restarting anyway'); return resolve(); }
            setTimeout(look, 500);
        };
        look();
    });
};

// Restart the player's backend a moment from now. The request goes to
// systemd as one restart job, which it carries out on its own once
// queued: a stop followed by a start from inside the service would end
// with the stop, since the stop takes every process of the service with
// it, the one waiting to start it again included.
Glass.prototype.restartBackend = function () {
    var self = this;
    self.logger.info(id + 'upgrade: restarting the backend');
    try {
        var child = require('child_process').spawn('/bin/sh', ['-c', 'sleep 3; sudo -n /bin/systemctl --no-block restart volumio'], { detached: true, stdio: 'ignore' });
        child.unref();
    } catch (e) {
        self.logger.error(id + 'upgrade: restart: ' + (e && e.message ? e.message : e));
    }
};

// What the status page shows about the plugin and the display.
// The fonts as the Status tab says them: each style's kind and name, and
// how many fonts were uploaded; no checksums, this is read often.
Glass.prototype.fontsSummary = function () {
    var current = (meterConfig && meterConfig.current) || {};
    var styles = {};
    FONT_STYLES.forEach(function (style) { styles[style] = fontChoice(current['font.' + style]); });
    var uploaded = 0;
    try { uploaded = fs.readdirSync(CustomFontsPath).filter(function (n) { return n.indexOf('.') !== 0 && isFontFile(n); }).length; } catch (e) {}
    return { styles: styles, uploaded: uploaded };
};

// Theme folders under a root: the ones with the file that makes a theme.
function themeCount(root, file) {
    try {
        return fs.readdirSync(root).filter(function (n) {
            return n.indexOf('.') !== 0 && fs.existsSync(root + '/' + n + '/' + file);
        }).length;
    } catch (e) { return 0; }
}

Glass.prototype.statusInfo = function () {
    var self = this;
    var arch = self.volumioArch();
    var state = self.lastState || {};
    self.loadConfigs();
    var meterBase = String(base_folder_P || (DATA_DIR + '/templates/')).replace(/\/$/, '');
    var spectrumBase = String(base_folder_S || (DATA_DIR + '/templates_spectrum/')).replace(/\/$/, '');
    var artwork = self.artworkSettings();
    var cachedArtists = 0;
    try { cachedArtists = fs.readdirSync(FanartCacheDir).filter(function (n) { return n.indexOf('.') !== 0 && fs.statSync(FanartCacheDir + '/' + n).isDirectory(); }).length; } catch (e) {}
    return {
        fonts: self.fontsSummary(),
        themes: { meterBase: meterBase, meters: themeCount(meterBase, 'meters.txt'), spectrumBase: spectrumBase, spectrum: themeCount(spectrumBase, 'spectrum.txt') },
        sharing: self.sharingInfo(),
        cardash: self.carDashInfo(),
        face: self.face ? self.face.status() : null,
        sheet: self.sheetInfo(),
        interactive: self.interactiveMode(),
        artwork: { enabled: artwork.enabled, keyMode: artwork.keyMode, interval: artwork.interval, order: artwork.order, cachedArtists: cachedArtists },
        version: pluginVersion,
        arch: arch,
        binary: fs.existsSync(PluginPath + '/bin/' + arch + '/glass'),
        running: fs.existsSync(runFlag),
        display: String(self.config.get('displayOutput') || '0'),
        headless: self.config.get('headless') === true,
        timeout: parseInt(self.config.get('timeout'), 10) || 0,
        activeTheme: self.activeTheme(),
        meter: self.meterSelection(),
        channel: {
            clients: self.channel ? self.channel.clients.length : 0,
            status: state.status || null,
            service: state.service || null,
            title: state.title || null,
            artist: state.artist || null
        },
        showing: self.channel && self.channel.showing ? self.channel.showing : null,
        logging: Object.assign({ levels: logging.LEVELS, targetsAvailable: logging.TARGETS }, self.logSettings()),
        performance: self.performanceInfo(),
        legacy: self.legacyEnabled(),
        language: self.managerLanguage()
    };
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
