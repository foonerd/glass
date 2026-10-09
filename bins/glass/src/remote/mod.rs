//! A remote display's own affairs: its configuration, its state for its
//! page, and the page itself, served from a thread of its own on a port
//! of its own. The frame loop never waits on any of this: the page reads a
//! snapshot and a change lands as a new generation number the loop looks
//! at once a frame.

pub mod config;
pub mod run;
pub mod signing;
pub mod upgrade;

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use config::{Player, RemoteConfig, ThemeChoice};
use intake::remote::{Beacon, DEFAULT_BEACON_PORT};

const PAGE: &str = include_str!("page.html");
/// The report's forms, one script with the Manager's page: compose, the
/// forum cut, the issue body, the file name.
const REPORT_JS: &str = include_str!("../../../../plugin/manager/report.js");

/// What the page shows about the display right now.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    /// `no player`, `syncing`, `showing`, `waiting`, or a problem in words.
    pub phase: String,
    pub player: String,
    pub host: String,
    pub connected: bool,
    pub frames_per_s: f32,
    pub received: u64,
    pub refused: u64,
    pub theme: String,
    pub meter: String,
    pub screen: [u32; 2],
    pub synced: String,
    pub player_release: String,
    pub since: String,
    /// The frame rate in force, which the governor may have lowered.
    pub rate: u32,
    /// This page's address on the network, once known.
    pub page: String,
    /// The screens this machine has, once the window opened.
    pub monitors: Vec<MonitorInfo>,
    /// Whether the face this display carries is drawn in this session.
    pub face_shown: bool,
    /// What draws the frames, once the window opened: the video driver, the
    /// renderer, and whether it draws in software.
    pub driver: String,
    pub renderer: String,
    pub software: bool,
    /// Whose the player's screen is, as the last sync heard it.
    pub face_owner: String,
}

/// What this machine has for the display to run, for the page's
/// Prerequisites panel and the support bundle.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Prerequisites {
    pub version: String,
    pub arch: String,
    pub os: String,
    pub exe: String,
    pub under_service: bool,
    pub previous_kept: bool,
    pub on_trial: bool,
    pub face: String,
    /// Files in the home's font folders: the plugin's, the player's web
    /// fonts, the uploaded ones.
    pub fonts: [usize; 3],
    pub config_path: String,
    pub cache_path: String,
    pub home_path: String,
    pub cache_writable: bool,
    /// Free bytes where the cache is; 0 where the system does not say.
    pub cache_free: u64,
}

/// Whether a folder takes a file now, by writing and removing one.
fn writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".glass-writable");
    match std::fs::write(&probe, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Free bytes on the file system under `dir`; 0 where it cannot be asked.
#[cfg(unix)]
fn free_bytes(dir: &std::path::Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return 0;
    };
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a C string that lives through the call and a struct the call fills.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return 0;
    }
    (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64)
}

#[cfg(not(unix))]
fn free_bytes(_dir: &std::path::Path) -> u64 {
    0
}

fn files_in(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

/// What the page's Connection panel shows: the paths to the player, each
/// tried from this side, and the probe of the frames path judged from
/// both ends. The words are the page's; this carries states and numbers.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Connection {
    /// Seconds since 1970 of the last Check now; 0 before one.
    pub checked_at: u64,
    /// The TCP paths: the Manager, the channel, the player's own web.
    pub tcp: Vec<TcpPath>,
    /// The frames path over UDP, judged by a probe.
    pub probe: Probe,
    /// The port this remote hears frames on, 0 before a session binds one.
    pub frames_port: u16,
    /// The player's ports as its beacon says them, for the rule table.
    pub player: PlayerPorts,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PlayerPorts {
    pub host: String,
    pub frames: u16,
    pub channel: u16,
    pub manager: u16,
    pub web: u16,
    pub beacon: u16,
}

/// One TCP path tried with a connect: `open`, `refused` (nothing listens
/// there), `timeout` (dropped on the way), `unreachable` (no route, or a
/// name that does not resolve), or `error` with the words.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TcpPath {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub state: String,
    pub detail: String,
    pub ms: u64,
}

/// The frames path's probe: `idle` (none asked), `running`, `arrived`
/// (UDP in is open), `blocked` (the player sent and nothing came),
/// `not-sent` (the player could not send: no port known, an older
/// player), `no-answer` (the channel took the request and said nothing),
/// `no-channel` (the request could not be sent).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Probe {
    pub state: String,
    pub sent: u32,
    pub arrived: u64,
    pub to: String,
    pub at: u64,
}

/// The probe's verdict from what arrived, what the player said it sent,
/// and whether the wait is over; none while there is still something to
/// wait for.
pub fn probe_verdict(arrived: u64, sent: Option<u32>, waited: bool) -> Option<&'static str> {
    if arrived > 0 {
        return Some("arrived");
    }
    if !waited {
        return None;
    }
    Some(match sent {
        None => "no-answer",
        Some(0) => "not-sent",
        Some(_) => "blocked",
    })
}

/// A TCP connect's outcome as a state word, from the error's kind.
pub fn tcp_state(err: &std::io::Error) -> &'static str {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::ConnectionRefused => "refused",
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "timeout",
        ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::AddrNotAvailable => "unreachable",
        _ => "error",
    }
}

/// Seconds since 1970, for the page's "when".
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One TCP path tried: a connect with a short timeout, the outcome and how
/// long it took.
pub fn try_tcp(name: &str, host: &str, port: u16, timeout: Duration) -> TcpPath {
    use std::net::ToSocketAddrs;
    let started = std::time::Instant::now();
    let addrs = match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(err) => {
            return TcpPath {
                name: name.to_string(),
                host: host.to_string(),
                port,
                state: "unreachable".to_string(),
                detail: format!("the name does not resolve: {err}"),
                ms: started.elapsed().as_millis() as u64,
            }
        }
    };
    let mut last: Option<std::io::Error> = None;
    for addr in addrs {
        match std::net::TcpStream::connect_timeout(&addr, timeout) {
            Ok(_) => {
                return TcpPath {
                    name: name.to_string(),
                    host: host.to_string(),
                    port,
                    state: "open".to_string(),
                    detail: String::new(),
                    ms: started.elapsed().as_millis() as u64,
                }
            }
            Err(err) => last = Some(err),
        }
    }
    let (state, detail) = match last {
        Some(err) => (tcp_state(&err), err.to_string()),
        None => ("unreachable", "the name resolves to no address".to_string()),
    };
    TcpPath {
        name: name.to_string(),
        host: host.to_string(),
        port,
        state: state.to_string(),
        detail,
        ms: started.elapsed().as_millis() as u64,
    }
}

/// One screen of this machine.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// The remote's state shared between the frame loop and the page.
pub struct RemoteApp {
    pub path: PathBuf,
    pub cache: PathBuf,
    config: Mutex<RemoteConfig>,
    generation: AtomicU64,
    status: Mutex<Status>,
    /// A window mode the page asked for, until the display takes it.
    window_request: Mutex<Option<pane::WindowMode>>,
    /// The face the display was built with, by its name; none on the
    /// standalone remote.
    face: Mutex<Option<String>>,
    /// What this display is a release of, where it can bring itself up to
    /// date, and where an upgrade stands.
    product: Mutex<Option<upgrade::Product>>,
    upgrade: Mutex<upgrade::State>,
    /// An upgrade is in place: the display is to become the new binary.
    restart: AtomicBool,
    /// The paths to the player as last tried, for the page's Connection panel.
    connection: Mutex<Connection>,
    /// The page asked for a probe of the frames path; the session takes it.
    probe_request: AtomicBool,
    /// The player's beacon of the session on, for the connection checks.
    beacon: Mutex<Option<Beacon>>,
    /// The theme's assets as the last sync left them, for the page.
    assets: Mutex<Option<intake::support::AssetsReport>>,
    /// The home of the session on: where the player's files are kept here.
    home: Mutex<Option<PathBuf>>,
}

impl RemoteApp {
    pub fn new(path: PathBuf, cache: PathBuf, config: RemoteConfig) -> Arc<Self> {
        Arc::new(Self {
            path,
            cache,
            config: Mutex::new(config),
            generation: AtomicU64::new(1),
            status: Mutex::new(Status::default()),
            window_request: Mutex::new(None),
            face: Mutex::new(None),
            product: Mutex::new(None),
            upgrade: Mutex::new(upgrade::State {
                phase: "idle".to_string(),
                ..Default::default()
            }),
            restart: AtomicBool::new(false),
            connection: Mutex::new(Connection {
                probe: Probe {
                    state: "idle".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            probe_request: AtomicBool::new(false),
            beacon: Mutex::new(None),
            assets: Mutex::new(None),
            home: Mutex::new(None),
        })
    }

    /// The theme's assets as the sync left them, and the home they are in.
    pub fn set_assets(&self, home: Option<PathBuf>, report: Option<intake::support::AssetsReport>) {
        *self.home.lock().unwrap_or_else(|e| e.into_inner()) = home;
        *self.assets.lock().unwrap_or_else(|e| e.into_inner()) = report;
    }

    pub fn assets(&self) -> Option<intake::support::AssetsReport> {
        self.assets
            .lock()
            .map(|a| a.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// What this machine has for the display to run, read now.
    pub fn prerequisites(&self) -> Prerequisites {
        let home = self.home.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let fonts = match &home {
            Some(home) => [
                files_in(&home.join("fonts")),
                files_in(&home.join("webfonts")),
                files_in(&home.join("customfonts")),
            ],
            None => [0, 0, 0],
        };
        let exe = std::env::current_exe().unwrap_or_default();
        let previous = exe.with_file_name(match exe.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.ends_with(".exe") => {
                format!("{}.prev.exe", name.trim_end_matches(".exe"))
            }
            Some(name) => format!("{name}.prev"),
            None => "glass.prev".to_string(),
        });
        Prerequisites {
            version: env!("CARGO_PKG_VERSION").to_string(),
            arch: std::env::consts::ARCH.to_string(),
            os: std::env::consts::OS.to_string(),
            exe: exe.to_string_lossy().into_owned(),
            under_service: upgrade::under_service(),
            previous_kept: previous.is_file(),
            on_trial: upgrade::on_trial(&self.cache),
            face: self.face().unwrap_or_default(),
            fonts,
            config_path: self.path.to_string_lossy().into_owned(),
            cache_path: self.cache.to_string_lossy().into_owned(),
            home_path: home
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_default(),
            cache_writable: self.cache.is_dir() && writable(&self.cache),
            cache_free: free_bytes(&self.cache),
        }
    }

    /// The player's beacon for the session on, said when a session starts.
    pub fn set_beacon(&self, beacon: Option<Beacon>) {
        let ports = beacon.as_ref().map(|b| PlayerPorts {
            host: b.address(),
            frames: b.frames_port,
            channel: b.channel_port,
            manager: b.manager_port,
            web: b.player_port,
            beacon: DEFAULT_BEACON_PORT,
        });
        *self.beacon.lock().unwrap_or_else(|e| e.into_inner()) = beacon;
        if let Some(ports) = ports {
            self.set_connection(|c| c.player = ports);
        }
    }

    pub fn beacon(&self) -> Option<Beacon> {
        self.beacon
            .lock()
            .map(|b| b.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// The support bundle: everything this remote knows of itself and of
    /// its player, for the page's report. The player's own sheet is asked
    /// of its Manager now, with a short limit; null where it does not
    /// answer, with the reason.
    pub fn support(&self) -> Value {
        let beacon = self.beacon();
        let (player_status, player_error) = match &beacon {
            Some(b) => {
                let url = format!("{}/api/status", b.manager_url());
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(5)))
                    .build()
                    .new_agent();
                match agent.get(&url).call() {
                    Ok(mut r) => match r.body_mut().read_to_string() {
                        Ok(text) => match serde_json::from_str::<Value>(&text) {
                            Ok(v) => (v, String::new()),
                            Err(e) => (Value::Null, format!("{url}: not JSON: {e}")),
                        },
                        Err(e) => (Value::Null, format!("{url}: {e}")),
                    },
                    Err(e) => (Value::Null, format!("{url}: {e}")),
                }
            }
            None => (Value::Null, "no player".to_string()),
        };
        json!({
            "at": now_secs(),
            "remote": {
                "name": self.config().name,
                "release": env!("CARGO_PKG_VERSION"),
                "protocol": tap::wire::PROTOCOL,
                "face": self.face(),
                "product": self.product(),
                "page": self.status().page,
            },
            "player": {
                "name": beacon.as_ref().map(|b| b.name.clone()),
                "host": beacon.as_ref().map(|b| b.host.clone()),
                "address": beacon.as_ref().map(|b| b.address()),
                "release": beacon.as_ref().map(|b| b.release.clone()),
                "status": player_status,
                "error": player_error,
            },
            "config": self.config(),
            "status": self.status(),
            "connection": self.connection(),
            "assets": self.assets(),
            "prerequisites": self.prerequisites(),
            "upgrade": self.upgrade(),
            "log": logline::recent(),
        })
    }

    pub fn connection(&self) -> Connection {
        self.connection
            .lock()
            .map(|c| c.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn set_connection(&self, change: impl FnOnce(&mut Connection)) {
        let mut guard = self.connection.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut guard);
    }

    /// The page asks for a probe of the frames path; the session takes it
    /// on its next step.
    pub fn request_probe(&self) {
        self.probe_request.store(true, Ordering::Release);
    }

    pub fn take_probe_request(&self) -> bool {
        self.probe_request.swap(false, Ordering::AcqRel)
    }

    /// Check now: the TCP paths tried from here with a three second limit
    /// each, kept for the page. Blocks for as long as the connects take;
    /// the route runs it on a thread of its own.
    pub fn check_connection(&self) {
        let Some(beacon) = self
            .beacon
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        let host = beacon.address();
        let limit = Duration::from_secs(3);
        let tcp = vec![
            try_tcp("manager", &host, beacon.manager_port, limit),
            try_tcp("channel", &host, beacon.channel_port, limit),
            try_tcp("web", &host, beacon.player_port, limit),
        ];
        let at = now_secs();
        self.set_connection(|c| {
            c.tcp = tcp;
            c.checked_at = at;
        });
    }

    /// What this display is a release of, said once at the start.
    pub fn set_product(&self, product: Option<upgrade::Product>) {
        *self.product.lock().unwrap_or_else(|e| e.into_inner()) = product;
    }

    pub fn product(&self) -> Option<upgrade::Product> {
        self.product
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Where an upgrade stands.
    pub fn upgrade(&self) -> upgrade::State {
        self.upgrade
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn set_upgrade(&self, change: impl FnOnce(&mut upgrade::State)) {
        change(&mut self.upgrade.lock().unwrap_or_else(|e| e.into_inner()));
    }

    /// Take the upgrade's one turn: true where nothing else is at it, and
    /// the phase is then `phase`.
    fn begin_upgrade(&self, phase: &str) -> bool {
        let mut state = self.upgrade.lock().unwrap_or_else(|e| e.into_inner());
        if state.phase != "idle" {
            return false;
        }
        state.phase = phase.to_string();
        state.error.clear();
        state.done = 0;
        true
    }

    /// Ask GitHub for the release this display is offered, the latest or
    /// with the test releases switch the newest of the last ten, and keep
    /// the answer for the page. Nothing where another look or an upgrade is
    /// under way, or the display is offered none.
    pub fn check_upgrade(&self) {
        let Some(product) = self.product() else {
            return;
        };
        if !self.begin_upgrade("checking") {
            return;
        }
        // The player's shelf first: a player with install from a file on
        // keeps archives for its remotes' machines and offers them here on
        // the network, so a remote that cannot reach GitHub still upgrades.
        let player = self
            .config()
            .player()
            .map(|(_, p)| (p.host.clone(), p.manager_port));
        let shelf = player.and_then(|(host, port)| {
            match upgrade::offered_by_player(&product, &host, port) {
                Ok(release) => {
                    logline::say!(
                        Info,
                        "remotes",
                        "upgrade: the player at {host} offers {} {} from its shelf",
                        product.name,
                        release.version
                    );
                    Some(release)
                }
                Err(why) => {
                    logline::say!(
                        Verbose,
                        "remotes",
                        "upgrade: nothing from the player at {host}: {why}"
                    );
                    None
                }
            }
        });
        let found = upgrade::prefer(
            shelf,
            upgrade::offered(&product, self.config().test_releases),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.set_upgrade(|state| {
            state.phase = "idle".to_string();
            state.checked_at = now;
            match found {
                Ok(release) => {
                    state.available = upgrade::newer(&release.version, &product.version);
                    state.latest = Some(release);
                }
                Err(why) => state.error = why,
            }
        });
    }

    /// An archive brought by hand, examined already, in place of what runs:
    /// tried and put in place as an upgrade from GitHub is; the frame loop
    /// is then asked to become it. The upgrade's turn is taken by the caller.
    pub fn install_from_file(&self, examined: upgrade::Examined, running: upgrade::Product) {
        let done = (|| {
            let exe =
                std::env::current_exe().map_err(|e| format!("this binary's own path: {e}"))?;
            upgrade::install_examined(&examined, &exe, &self.cache, &running)
        })();
        match done {
            Ok(()) => {
                logline::say!(
                    Info,
                    "remotes",
                    "upgrade: {} {} from a file is in place of {} {}, verified by the signature it carries ({} files); starting again as it",
                    examined.product,
                    examined.version,
                    running.name,
                    running.version,
                    examined.files
                );
                self.set_upgrade(|state| state.phase = "restarting".to_string());
                self.restart.store(true, Ordering::Release);
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
            Err(why) => {
                logline::say!(Info, "remotes", "upgrade from a file: not done: {why}");
                self.set_upgrade(|state| {
                    state.phase = "idle".to_string();
                    state.error = why;
                });
            }
        }
    }

    /// Bring the display up to the latest release known: fetched, checked,
    /// tried and put in place; the frame loop is then asked to become it.
    /// What fails is said in the state, and the display runs on as it is.
    pub fn run_upgrade(&self) {
        let (Some(product), Some(release)) = (self.product(), self.upgrade().latest) else {
            return;
        };
        if !upgrade::newer(&release.version, &product.version) || !self.begin_upgrade("downloading")
        {
            return;
        }
        let done = (|| {
            let archive = upgrade::download(&release, |bytes| {
                self.set_upgrade(|state| state.done = bytes);
            })?;
            self.set_upgrade(|state| state.phase = "installing".to_string());
            let paths = upgrade::binary_paths(&product.binary, upgrade::arch_folders());
            let binary = upgrade::binary_from(&archive, &paths)?;
            let exe =
                std::env::current_exe().map_err(|e| format!("this binary's own path: {e}"))?;
            upgrade::install(&binary, &exe, &self.cache, &product, &release)
        })();
        match done {
            Ok(()) => {
                logline::say!(
                    Info,
                    "remotes",
                    "upgrade: {} {} is in place of {}; starting again as it",
                    product.name,
                    release.version,
                    product.version
                );
                self.set_upgrade(|state| state.phase = "restarting".to_string());
                self.restart.store(true, Ordering::Release);
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
            Err(why) => {
                logline::say!(Info, "remotes", "upgrade: not done: {why}");
                self.set_upgrade(|state| {
                    state.phase = "idle".to_string();
                    state.error = why;
                });
            }
        }
    }

    /// Whether an upgrade is in place and the display is to become it.
    pub fn restart_asked(&self) -> bool {
        self.restart.load(Ordering::Acquire)
    }

    /// The face the display was built with, said once at the start.
    pub fn set_face(&self, name: Option<String>) {
        *self.face.lock().unwrap_or_else(|e| e.into_inner()) = name;
    }

    /// The face this display carries, by its name.
    pub fn face(&self) -> Option<String> {
        self.face.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn config(&self) -> RemoteConfig {
        self.config
            .lock()
            .map(|c| c.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// The number that grows with every change; the frame loop compares.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Change the configuration, check it, keep it, and say so.
    pub fn update(&self, change: impl FnOnce(&mut RemoteConfig)) -> Result<RemoteConfig, String> {
        let mut guard = self.config.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        change(&mut next);
        next.tidy();
        next.check()?;
        next.save(&self.path)?;
        *guard = next.clone();
        self.generation.fetch_add(1, Ordering::AcqRel);
        apply_log_level(&next);
        Ok(next)
    }

    /// Ask the display to change how its window sits, live.
    pub fn request_window(&self, mode: pane::WindowMode) {
        *self
            .window_request
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(mode);
    }

    /// The mode the page asked for, once.
    pub fn take_window_request(&self) -> Option<pane::WindowMode> {
        self.window_request
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    pub fn set_status(&self, change: impl FnOnce(&mut Status)) {
        let mut guard = self.status.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut guard);
    }

    pub fn set_phase(&self, phase: &str) {
        self.set_status(|s| s.phase = phase.to_string());
    }
}

/// Serve the page on the configured port from a thread of its own.
/// Returns the port it listens on, or why it could not.
pub fn serve(app: Arc<RemoteApp>) -> Result<u16, String> {
    let port = app.config().page_port;
    let server = Server::http(("0.0.0.0", port)).map_err(|e| format!("page port {port}: {e}"))?;
    std::thread::Builder::new()
        .name("glass-page".into())
        .spawn(move || {
            for request in server.incoming_requests() {
                let app = app.clone();
                // Each request on its own thread: discovery takes seconds.
                std::thread::spawn(move || handle(&app, request));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(port)
}

/// The configuration's log level in force, or the environment's without one.
pub fn apply_log_level(config: &RemoteConfig) {
    match config.log_level.as_deref().and_then(logline::Level::parse) {
        Some(level) => logline::set_level(level),
        None => logline::clear_level(),
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a plain header")
}

fn respond_json(request: Request, status: u16, value: Value) {
    let body = value.to_string();
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-cache"));
    let _ = request.respond(response);
}

fn respond_error(request: Request, status: u16, code: &str, message: impl Into<String>) {
    respond_json(
        request,
        status,
        json!({ "error": code, "message": message.into() }),
    );
}

fn read_body(request: &mut Request) -> Result<Value, String> {
    let mut text = String::new();
    // At most a megabyte: a configuration is a few kilobytes.
    let reader: &mut dyn Read = request.as_reader();
    Read::take(reader, 1 << 20)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn query(url: &str, key: &str) -> Option<String> {
    let (_, q) = url.split_once('?')?;
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = &text[i + 1..i + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(app: &Arc<RemoteApp>, mut request: Request) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();
    if path != "/" && path != "/index.html" {
        logline::say!(
            Verbose,
            "remotes",
            "page: {method} {url} from {}",
            request
                .remote_addr()
                .map(|a| a.to_string())
                .unwrap_or_default()
        );
    }
    match (method, path.as_str()) {
        (Method::Get, "/") | (Method::Get, "/index.html") => {
            let response = Response::from_string(PAGE)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-cache"));
            let _ = request.respond(response);
        }
        (Method::Get, "/api/state") => {
            let config = app.config();
            respond_json(
                request,
                200,
                json!({
                    "config": config,
                    "status": app.status(),
                    "release": env!("CARGO_PKG_VERSION"),
                    "protocol": tap::wire::PROTOCOL,
                    "face": app.face(),
                    "product": app.product(),
                    "upgrade": app.upgrade(),
                    "upgradeInPlace": upgrade::in_place(),
                    "configPath": app.path.to_string_lossy(),
                    "cache": app.cache.to_string_lossy(),
                    "connection": app.connection(),
                    "exe": std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
                    "assets": app.assets(),
                    "prerequisites": app.prerequisites(),
                }),
            );
        }
        // The support bundle for the page's report, and the report's forms.
        (Method::Get, "/api/support") => {
            respond_json(request, 200, app.support());
        }
        (Method::Get, "/report.js") => {
            let response = Response::from_string(REPORT_JS)
                .with_header(header(
                    "Content-Type",
                    "application/javascript; charset=utf-8",
                ))
                .with_header(header("Cache-Control", "no-cache"));
            let _ = request.respond(response);
        }
        // Check now on the Connection panel: the TCP paths tried on a thread
        // of their own, and the frames path probed by the session through
        // the channel. Answers at once; the state says how it went.
        (Method::Post, "/api/connection/check") => {
            app.request_probe();
            let app = app.clone();
            std::thread::spawn(move || app.check_connection());
            respond_json(request, 200, json!({ "ok": true }));
        }
        // The display's own upgrade: a look at the releases, and the
        // upgrade to the latest one known. Neither takes anything from the
        // request: what is fetched is fixed by what the display is. Both
        // answer at once; the state says how it goes.
        (Method::Post, "/api/upgrade/check") | (Method::Post, "/api/upgrade/install") => {
            if app.product().is_none() {
                return respond_error(
                    request,
                    400,
                    "no-upgrade",
                    "this display is offered no upgrade",
                );
            }
            let state = app.upgrade();
            if state.phase != "idle" {
                return respond_error(request, 409, "busy", "an upgrade is under way");
            }
            let install = path == "/api/upgrade/install";
            if install && !upgrade::in_place() {
                return respond_error(
                    request,
                    400,
                    "by-hand",
                    "on this system the release is installed by the system's installer: open the release's package",
                );
            }
            if install && !state.available {
                return respond_error(
                    request,
                    400,
                    "up-to-date",
                    "no later release is known; check first",
                );
            }
            let worker = app.clone();
            std::thread::spawn(move || {
                if install {
                    worker.run_upgrade();
                } else {
                    worker.check_upgrade();
                }
            });
            respond_json(request, 202, json!({ "ok": true }));
        }
        // A release archive brought by hand, for a remote that cannot reach
        // GitHub: held to the signature it carries inside, with no network;
        // of this remote's kind, or of the other kind when `switch=1` says
        // so, which replaces this remote with that kind. Refused while the
        // switch "Install from a file" is off.
        (Method::Post, "/api/upgrade/upload") => {
            if !app.config().upload_install {
                return respond_error(
                    request,
                    403,
                    "switch-off",
                    "install from a file is off on this remote",
                );
            }
            let Some(running) = app.product() else {
                return respond_error(
                    request,
                    400,
                    "no-upgrade",
                    "this display is offered no upgrade",
                );
            };
            if !upgrade::in_place() {
                return respond_error(
                    request,
                    400,
                    "by-hand",
                    "on this system the release is installed by the system's installer: open the release's package",
                );
            }
            if app.upgrade().phase != "idle" {
                return respond_error(request, 409, "busy", "an upgrade is under way");
            }
            let switch = query(&url, "switch").as_deref() == Some("1");
            let mut archive = Vec::new();
            let reader: &mut dyn Read = request.as_reader();
            if let Err(e) = Read::take(reader, upgrade::MAX_ARCHIVE + 1).read_to_end(&mut archive) {
                return respond_error(request, 400, "upload", format!("the upload broke: {e}"));
            }
            if archive.len() as u64 > upgrade::MAX_ARCHIVE {
                return respond_error(
                    request,
                    413,
                    "too-large",
                    "the file is larger than a release archive",
                );
            }
            if archive.is_empty() {
                return respond_error(request, 400, "empty", "the upload is empty");
            }
            let key = match signing::public_key() {
                Ok(key) => key,
                Err(why) => return respond_error(request, 500, "key", &why),
            };
            let examined = match upgrade::examine_archive(&archive, &key, upgrade::arch_folders()) {
                Ok(examined) => examined,
                Err(why) => return respond_error(request, 400, "signature", &why),
            };
            if examined.product != running.name && !switch {
                return respond_error(
                    request,
                    409,
                    "other-product",
                    format!(
                        "the archive is {} {}; this remote is {} {}. Say so to replace it",
                        examined.product, examined.version, running.name, running.version
                    ),
                );
            }
            if examined.version == running.version && examined.product == running.name {
                return respond_error(
                    request,
                    400,
                    "same-version",
                    format!(
                        "{} {} is what runs; nothing to install",
                        running.name, running.version
                    ),
                );
            }
            if !app.begin_upgrade("installing") {
                return respond_error(request, 409, "busy", "an upgrade is under way");
            }
            let answer = json!({ "ok": true, "product": examined.product, "version": examined.version, "files": examined.files });
            let worker = app.clone();
            std::thread::spawn(move || worker.install_from_file(examined, running));
            respond_json(request, 202, answer);
        }
        // The window, live: full screen, or a window so the desktop behind
        // it can be used (Escape and F on the keyboard do the same).
        (Method::Post, "/api/window") => {
            let body = match read_body(&mut request) {
                Ok(v) => v,
                Err(e) => return respond_error(request, 400, "bad-json", e),
            };
            let mode = match body.get("mode").and_then(Value::as_str) {
                Some("fullscreen") => pane::WindowMode::Fullscreen,
                Some("windowed") => pane::WindowMode::Windowed,
                Some("frameless") => pane::WindowMode::Frameless,
                _ => {
                    return respond_error(
                        request,
                        400,
                        "bad-mode",
                        "mode: fullscreen, windowed or frameless",
                    )
                }
            };
            app.request_window(mode);
            respond_json(request, 200, json!({ "ok": true }));
        }
        (Method::Get, "/api/config/export") => {
            let config = app.config();
            let name: String = config
                .name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let file = format!(
                "glass-remote-{}.json",
                if name.is_empty() { "config" } else { &name }
            );
            let body = serde_json::to_string_pretty(&config).unwrap_or_default();
            let response = Response::from_string(body)
                .with_header(header("Content-Type", "application/json; charset=utf-8"))
                .with_header(header(
                    "Content-Disposition",
                    &format!("attachment; filename=\"{file}\""),
                ))
                .with_header(header("Cache-Control", "no-cache"));
            let _ = request.respond(response);
        }
        // A configuration file from this or another remote: checked and
        // kept whole, its folder paths as they are.
        (Method::Post, "/api/config/import") => {
            let body = match read_body(&mut request) {
                Ok(v) => v,
                Err(e) => return respond_error(request, 400, "bad-json", e),
            };
            let incoming: RemoteConfig = match serde_json::from_value(body) {
                Ok(c) => c,
                Err(e) => return respond_error(request, 400, "bad-config", e.to_string()),
            };
            match app.update(|c| *c = incoming) {
                Ok(config) => respond_json(request, 200, json!({ "ok": true, "config": config })),
                Err(e) => respond_error(request, 400, "refused", e),
            }
        }
        (Method::Post, "/api/config") => {
            let body = match read_body(&mut request) {
                Ok(v) => v,
                Err(e) => return respond_error(request, 400, "bad-json", e),
            };
            let incoming: RemoteConfig = match serde_json::from_value(body) {
                Ok(c) => c,
                Err(e) => return respond_error(request, 400, "bad-config", e.to_string()),
            };
            match app.update(|c| *c = incoming) {
                Ok(config) => respond_json(request, 200, json!({ "ok": true, "config": config })),
                Err(e) => respond_error(request, 400, "refused", e),
            }
        }
        (Method::Post, "/api/player") => {
            let body = match read_body(&mut request) {
                Ok(v) => v,
                Err(e) => return respond_error(request, 400, "bad-json", e),
            };
            let host = body
                .get("host")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if host.is_empty() {
                return respond_error(request, 400, "no-host", "a host name or address is needed");
            }
            let manager_port = body
                .get("manager_port")
                .and_then(Value::as_u64)
                .filter(|p| (1..=65535).contains(p))
                .map(|p| p as u16)
                .unwrap_or(intake::remote::DEFAULT_MANAGER_PORT);
            // The player's manager names it and says its ports; without an
            // answer the player is kept as typed and said to be unreached.
            let (beacon, note) = Beacon::ask_manager(&host, manager_port);
            let name = body
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from)
                .unwrap_or_else(|| beacon.name.clone());
            let result = app.update(|c| {
                c.put_player(Player {
                    host: host.clone(),
                    manager_port,
                    name: name.clone(),
                    theme: ThemeChoice::Follow { same_meter: true },
                });
            });
            match result {
                Ok(config) => respond_json(
                    request,
                    200,
                    json!({ "ok": true, "config": config, "reached": note.is_none(), "note": note, "release": beacon.release }),
                ),
                Err(e) => respond_error(request, 400, "refused", e),
            }
        }
        (Method::Post, "/api/active") => {
            let body = match read_body(&mut request) {
                Ok(v) => v,
                Err(e) => return respond_error(request, 400, "bad-json", e),
            };
            let id = body
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let result = app.update(|c| {
                if c.players.contains_key(&id) {
                    c.active = Some(id.clone());
                }
            });
            match result {
                Ok(config) if config.active.as_deref() == Some(id.as_str()) => {
                    respond_json(request, 200, json!({ "ok": true, "config": config }))
                }
                Ok(_) => respond_error(request, 404, "not-found", "no such player"),
                Err(e) => respond_error(request, 400, "refused", e),
            }
        }
        (Method::Delete, p) if p.starts_with("/api/player/") => {
            let id = percent_decode(&p["/api/player/".len()..]);
            let result = app.update(|c| {
                c.players.remove(&id);
            });
            match result {
                Ok(config) => respond_json(request, 200, json!({ "ok": true, "config": config })),
                Err(e) => respond_error(request, 400, "refused", e),
            }
        }
        (Method::Get, "/api/discover") => {
            let seconds = query(&url, "seconds")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(6)
                .clamp(2, 30);
            match intake::remote::discover(DEFAULT_BEACON_PORT, Duration::from_secs(seconds)) {
                Ok(list) => {
                    let players: Vec<Value> = list
                        .iter()
                        .map(|b| {
                            json!({
                                "name": b.name, "host": b.address(), "hostname": b.host, "release": b.release,
                                "theme": b.theme, "manager_port": b.manager_port,
                            })
                        })
                        .collect();
                    respond_json(
                        request,
                        200,
                        json!({ "players": players, "seconds": seconds }),
                    )
                }
                Err(e) => respond_error(
                    request,
                    502,
                    "discovery",
                    format!("port {DEFAULT_BEACON_PORT}: {e}"),
                ),
            }
        }
        (Method::Get, "/api/local/themes") => {
            let dir = query(&url, "dir")
                .filter(|d| !d.trim().is_empty())
                .or_else(|| app.config().themes_dir)
                .unwrap_or_default();
            if dir.trim().is_empty() {
                return respond_error(request, 400, "no-folder", "no themes folder named");
            }
            match config::local_themes(&dir) {
                Ok(themes) => {
                    let (templates, spectrum) = config::local_roots(&dir);
                    respond_json(
                        request,
                        200,
                        json!({
                            "dir": dir,
                            "templates": templates.to_string_lossy(),
                            "spectrum": spectrum.to_string_lossy(),
                            "themes": themes,
                        }),
                    );
                }
                Err(e) => respond_error(request, 400, "not-a-folder", e),
            }
        }
        (Method::Get, "/api/player/themes") => {
            let config = app.config();
            let wanted = query(&url, "id");
            let player = match wanted {
                Some(id) => config.players.get(&id).cloned(),
                None => config.player().map(|(_, p)| p.clone()),
            };
            let Some(player) = player else {
                return respond_error(request, 404, "not-found", "no such player");
            };
            let target = format!("http://{}:{}/api/themes", player.host, player.manager_port);
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(8)))
                .build()
                .new_agent();
            let answer = agent
                .get(&target)
                .call()
                .map_err(|e| e.to_string())
                .and_then(|mut r| r.body_mut().read_to_string().map_err(|e| e.to_string()))
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()));
            match answer {
                Ok(value) => {
                    let themes: Vec<Value> = value
                        .get("themes")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter(|t| t.get("empty").and_then(Value::as_bool) != Some(true))
                                .map(|t| {
                                    json!({
                                        "folder": t.get("folder"), "meters": t.get("meters"), "spectrum": t.get("spectrum"),
                                        "width": t.get("width"), "height": t.get("height"), "active": t.get("active"),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    respond_json(
                        request,
                        200,
                        json!({ "themes": themes, "active": value.get("active") }),
                    )
                }
                Err(e) => respond_error(request, 502, "player", format!("{target}: {e}")),
            }
        }
        (Method::Options, _) => {
            let _ = request.respond(Response::empty(204));
        }
        _ => respond_error(request, 404, "not-found", "no such route"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_and_percent_escapes_are_read() {
        assert_eq!(
            query("/api/discover?seconds=9&x=1", "seconds").as_deref(),
            Some("9")
        );
        assert_eq!(query("/api/discover", "seconds"), None);
        assert_eq!(percent_decode("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
        assert_eq!(percent_decode("tail%2"), "tail%2");
    }

    #[test]
    fn a_change_grows_the_generation_and_a_bad_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("glass-remote-app-{}", std::process::id()));
        let app = RemoteApp::new(
            dir.join("config.json"),
            dir.join("cache"),
            RemoteConfig::default(),
        );
        let before = app.generation();
        app.update(|c| c.gain_db = 2.0).unwrap();
        assert_eq!(app.generation(), before + 1);
        assert_eq!(app.config().gain_db, 2.0);
        let refused = app.update(|c| {
            c.players.insert(
                "bad id!".into(),
                Player {
                    host: "x".into(),
                    manager_port: 5582,
                    name: String::new(),
                    theme: ThemeChoice::Follow { same_meter: true },
                },
            );
        });
        assert!(refused.is_err());
        assert_eq!(
            app.generation(),
            before + 1,
            "a refused change is no change"
        );
        assert!(app.config().players.is_empty());
        app.set_phase("showing");
        assert_eq!(app.status().phase, "showing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The probe's verdict rests on both ends: anything arrived is open at
    /// once; nothing arrived is judged only when the wait is over, by what
    /// the player said it sent.
    #[test]
    fn the_probe_is_judged_from_both_ends() {
        assert_eq!(probe_verdict(1, None, false), Some("arrived"));
        assert_eq!(probe_verdict(2, Some(3), true), Some("arrived"));
        assert_eq!(
            probe_verdict(0, Some(3), false),
            None,
            "still waiting for the datagrams"
        );
        assert_eq!(
            probe_verdict(0, None, false),
            None,
            "still waiting for the player"
        );
        assert_eq!(probe_verdict(0, Some(3), true), Some("blocked"));
        assert_eq!(probe_verdict(0, Some(0), true), Some("not-sent"));
        assert_eq!(probe_verdict(0, None, true), Some("no-answer"));
    }

    /// A connect's failure is one of four words, by its kind.
    #[test]
    fn a_tcp_failure_is_named_by_its_kind() {
        use std::io::{Error, ErrorKind};
        assert_eq!(
            tcp_state(&Error::from(ErrorKind::ConnectionRefused)),
            "refused"
        );
        assert_eq!(tcp_state(&Error::from(ErrorKind::TimedOut)), "timeout");
        assert_eq!(
            tcp_state(&Error::from(ErrorKind::HostUnreachable)),
            "unreachable"
        );
        assert_eq!(tcp_state(&Error::from(ErrorKind::Other)), "error");
        // A port nothing listens on, on this machine: refused, quickly.
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        let path = try_tcp("channel", "127.0.0.1", port, Duration::from_secs(2));
        assert_eq!(
            (path.name.as_str(), path.state.as_str(), path.port),
            ("channel", "refused", port)
        );
        // One that listens: open.
        let open = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = open.local_addr().unwrap().port();
        let path = try_tcp("manager", "127.0.0.1", port, Duration::from_secs(2));
        assert_eq!(path.state, "open");
        // A name that does not resolve: unreachable, with the words.
        let path = try_tcp("web", "no-such-host.invalid", 3000, Duration::from_secs(2));
        assert_eq!(path.state, "unreachable");
        assert!(path.detail.contains("resolve"));
    }
}
