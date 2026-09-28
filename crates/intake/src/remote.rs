//! A remote display's side of the line: the frames over UDP, the player
//! found by its beacon, and the player's configuration, theme, fonts and
//! icons brought into a home directory of the display's own, so the
//! display reads them as it reads the player's.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tap::wire;

pub use crate::bring::{
    asset_plan, config_texts, custom_font_keys, encode, rewrite_config, theme_plan, Asset, Bring,
    Choice, LocalThemes, RemoteAssets, RemoteConfig, RemoteFiles, Texts, ThemeFile, ThemeFiles,
    ThemeTree,
};
use crate::hops::WireHops;
use crate::{Hops, Taken};

pub const DEFAULT_FRAMES_PORT: u16 = 5580;
pub const DEFAULT_CHANNEL_PORT: u16 = 5581;
pub const DEFAULT_MANAGER_PORT: u16 = 5582;
pub const DEFAULT_BEACON_PORT: u16 = 5579;
pub const DEFAULT_PLAYER_PORT: u16 = 3000;
/// A subscribe goes to the player this often; it forgets a remote after fifteen seconds.
const SUBSCRIBE_EVERY: Duration = Duration::from_secs(5);
const DATAGRAM_MAX: usize = 8192;

/// The frames as they arrive from the player over UDP, merged between
/// two looks; the decoding, the ordering, the gain and the decay are
/// [`WireHops`]'s.
pub struct NetHops {
    socket: UdpSocket,
    server: SocketAddr,
    subscribe: Vec<u8>,
    subscribed_at: Option<Instant>,
    buffer: Vec<u8>,
    wire: WireHops,
}

impl NetHops {
    /// Subscribes to the player's frames port from a port of this host's own.
    pub fn new(server: SocketAddr, id: &str, name: &str, release: &str) -> std::io::Result<Self> {
        let bind: SocketAddr = if server.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let socket = UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        let subscribe = serde_json::json!({
            "glass": "subscribe",
            "protocol": wire::PROTOCOL,
            "id": id,
            "name": name,
            "release": release,
        })
        .to_string()
        .into_bytes();
        Ok(Self {
            socket,
            server,
            subscribe,
            subscribed_at: None,
            buffer: vec![0u8; DATAGRAM_MAX],
            wire: WireHops::new(),
        })
    }

    pub fn server(&self) -> SocketAddr {
        self.server
    }

    /// Scale the levels by a gain in decibels, within plus or minus twelve.
    pub fn with_gain_db(mut self, db: f32) -> Self {
        self.wire = std::mem::take(&mut self.wire).with_gain_db(db);
        self
    }

    /// Let a spectrum bar fall by at most this share of its height per
    /// frame, 0.5 to 0.99 (Peppy Remote's decay rate); 0 turns it off.
    pub fn with_spectrum_decay(mut self, per_frame: f32) -> Self {
        self.wire = std::mem::take(&mut self.wire).with_spectrum_decay(per_frame);
        self
    }

    fn subscribe_if_due(&mut self) {
        let due = self
            .subscribed_at
            .is_none_or(|at| at.elapsed() >= SUBSCRIBE_EVERY);
        if due {
            self.subscribed_at = Some(Instant::now());
            let _ = self.socket.send_to(&self.subscribe, self.server);
        }
    }
}

impl Hops for NetHops {
    fn take(&mut self) -> Taken {
        self.subscribe_if_due();
        loop {
            match self.socket.recv_from(&mut self.buffer) {
                Ok((n, from)) => {
                    if from.ip() == self.server.ip() {
                        self.wire.push(&self.buffer[..n]);
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        self.wire.take()
    }

    fn stats(&self) -> (u64, u64) {
        self.wire.stats()
    }
}

/// A player as its beacon announces it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Beacon {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_frames_port")]
    pub frames_port: u16,
    #[serde(default = "default_channel_port")]
    pub channel_port: u16,
    #[serde(default = "default_manager_port")]
    pub manager_port: u16,
    #[serde(default = "default_player_port")]
    pub player_port: u16,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub config: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub meter: String,
    /// Where the beacon came from, which is the address to use.
    #[serde(skip)]
    pub from: Option<IpAddr>,
}

fn default_frames_port() -> u16 {
    DEFAULT_FRAMES_PORT
}
fn default_channel_port() -> u16 {
    DEFAULT_CHANNEL_PORT
}
fn default_manager_port() -> u16 {
    DEFAULT_MANAGER_PORT
}
fn default_player_port() -> u16 {
    DEFAULT_PLAYER_PORT
}

impl Beacon {
    /// A beacon for a player named by hand, with the default ports.
    pub fn named(host: &str) -> Self {
        Self {
            name: host.to_string(),
            host: host.to_string(),
            frames_port: DEFAULT_FRAMES_PORT,
            channel_port: DEFAULT_CHANNEL_PORT,
            manager_port: DEFAULT_MANAGER_PORT,
            player_port: DEFAULT_PLAYER_PORT,
            ..Default::default()
        }
    }

    /// The address a remote talks to: where the beacon came from, else the host named.
    pub fn address(&self) -> String {
        match self.from {
            Some(ip) => ip.to_string(),
            None => self.host.clone(),
        }
    }

    pub fn manager_url(&self) -> String {
        format!("http://{}:{}", self.address(), self.manager_port)
    }

    /// One beacon datagram, or none when it is not a player's.
    pub fn parse(bytes: &[u8], from: IpAddr) -> Option<Self> {
        let value: Value = serde_json::from_slice(bytes).ok()?;
        if value.get("glass")?.as_str()? != "player" {
            return None;
        }
        if value.get("protocol").and_then(Value::as_u64).unwrap_or(0) != wire::PROTOCOL as u64 {
            return None;
        }
        let mut beacon: Beacon = serde_json::from_value(value).ok()?;
        beacon.from = Some(from);
        Some(beacon)
    }
}

impl Beacon {
    /// A player named by hand, its ports asked of its manager: what
    /// `/api/remote/status` says, or the defaults when it does not answer.
    pub fn ask_manager(host: &str, manager_port: u16) -> (Self, Option<String>) {
        let url = format!("http://{host}:{manager_port}/api/remote/status");
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .new_agent();
        let answer = agent
            .get(&url)
            .call()
            .map_err(|e| e.to_string())
            .and_then(|mut r| r.body_mut().read_to_string().map_err(|e| e.to_string()))
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()));
        match answer {
            Ok(value) => (Self::from_status(&value, host, manager_port), None),
            Err(err) => (
                Self {
                    manager_port,
                    ..Self::named(host)
                },
                Some(format!("{url}: {err}")),
            ),
        }
    }

    /// The manager's `/api/remote/status` as a beacon for `host`.
    pub fn from_status(status: &Value, host: &str, manager_port: u16) -> Self {
        let ports = status.get("ports").cloned().unwrap_or(Value::Null);
        let port = |key: &str, fallback: u16| {
            ports
                .get(key)
                .and_then(Value::as_u64)
                .filter(|p| (1..=65535).contains(p))
                .map(|p| p as u16)
                .unwrap_or(fallback)
        };
        let beacon = status.get("beacon").cloned().unwrap_or(Value::Null);
        let text = |key: &str| {
            beacon
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        Self {
            name: if text("name").is_empty() {
                host.to_string()
            } else {
                text("name")
            },
            host: host.to_string(),
            frames_port: port("frames", DEFAULT_FRAMES_PORT),
            channel_port: port("channel", DEFAULT_CHANNEL_PORT),
            manager_port: port("manager", manager_port),
            player_port: beacon
                .get("player_port")
                .and_then(Value::as_u64)
                .map(|p| p as u16)
                .unwrap_or(DEFAULT_PLAYER_PORT),
            release: text("release"),
            config: text("config"),
            theme: text("theme"),
            meter: text("meter"),
            from: None,
        }
    }
}

/// Listen for players' beacons for `wait`, one entry per player.
pub fn discover(port: u16, wait: Duration) -> std::io::Result<Vec<Beacon>> {
    let socket = UdpSocket::bind(("0.0.0.0", port))?;
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    let started = Instant::now();
    let mut found: HashMap<IpAddr, Beacon> = HashMap::new();
    let mut buffer = [0u8; DATAGRAM_MAX];
    while started.elapsed() < wait {
        match socket.recv_from(&mut buffer) {
            Ok((n, from)) => {
                if let Some(beacon) = Beacon::parse(&buffer[..n], from.ip()) {
                    found.insert(from.ip(), beacon);
                }
            }
            Err(err)
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut => {}
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    let mut list: Vec<Beacon> = found.into_values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// The player's frames port as a socket address, the host resolved.
pub fn frames_address(beacon: &Beacon) -> std::io::Result<SocketAddr> {
    (beacon.address().as_str(), beacon.frames_port)
        .to_socket_addrs()?
        .find(|a| a.is_ipv4())
        .or_else(|| {
            (beacon.address().as_str(), beacon.frames_port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut a| a.next())
        })
        .ok_or_else(|| {
            std::io::Error::new(ErrorKind::NotFound, "the player's address does not resolve")
        })
}

/// The address this host reaches `player` from: what the player sees as
/// the remote's address, and what a browser on the same network reaches.
pub fn own_address_towards(player: &str, port: u16) -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect((player, port)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// This host's own IPv4 addresses, loopback left out: on Linux from the
/// kernel's routing trie, on Android from the interface list (the routing
/// table is closed to applications there), elsewhere the address a route
/// to the outside would leave from.
pub fn own_addresses() -> Vec<std::net::Ipv4Addr> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/net/fib_trie")
            .map(|text| local_addresses_in(&text))
            .unwrap_or_default()
    }
    #[cfg(target_os = "android")]
    {
        interface_addresses()
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        match own_address_towards("192.0.2.1", 9) {
            Some(IpAddr::V4(ip)) if !ip.is_loopback() => vec![ip],
            _ => Vec::new(),
        }
    }
}

/// The IPv4 address of every interface that is up, loopback left out,
/// from the C library's interface list.
#[cfg(target_os = "android")]
fn interface_addresses() -> Vec<std::net::Ipv4Addr> {
    let mut found = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills a list the C library owns until freeifaddrs;
    // every pointer is checked before it is read, and the list is walked
    // once and freed once.
    unsafe {
        if libc::getifaddrs(&mut head) != 0 {
            return found;
        }
        let mut cur = head;
        while !cur.is_null() {
            let entry = &*cur;
            if !entry.ifa_addr.is_null()
                && i32::from((*entry.ifa_addr).sa_family) == libc::AF_INET
                && (entry.ifa_flags & libc::IFF_UP as u32) != 0
            {
                let sin = &*(entry.ifa_addr as *const libc::sockaddr_in);
                let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                if !ip.is_loopback() && !found.contains(&ip) {
                    found.push(ip);
                }
            }
            cur = entry.ifa_next;
        }
        libc::freeifaddrs(head);
    }
    found
}

/// The `/32 host LOCAL` entries of a routing trie dump: each is an address
/// of this host's own, named on the line before its `/32` line.
#[cfg(any(target_os = "linux", test))]
fn local_addresses_in(trie: &str) -> Vec<std::net::Ipv4Addr> {
    let mut found: Vec<std::net::Ipv4Addr> = Vec::new();
    let mut last: Option<std::net::Ipv4Addr> = None;
    for line in trie.lines() {
        let line = line.trim_start_matches(['|', '+', '-', ' ']);
        if let Some(rest) = line.strip_prefix("/32 host LOCAL") {
            let _ = rest;
            if let Some(ip) = last.take() {
                if !ip.is_loopback() && !found.contains(&ip) {
                    found.push(ip);
                }
            }
        } else if let Some((address, _)) = line.split_once('/') {
            last = address.trim().parse().ok();
        } else {
            last = line.trim().parse().ok();
        }
    }
    found
}

/// What a sync brought or confirmed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Synced {
    pub version: String,
    pub theme: String,
    pub meter: String,
    pub fetched: usize,
    pub kept: usize,
}

/// What was fetched before, by path under the home, with its checksum.
#[derive(Serialize, Deserialize, Default)]
struct Ledger {
    #[serde(default)]
    version: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    files: HashMap<String, String>,
}

/// Brings the player's configuration and assets into `home`.
pub struct Sync {
    pub home: PathBuf,
    pub manager: String,
    pub player: String,
    agent: ureq::Agent,
    ledger: Ledger,
    log: Vec<String>,
}

impl Sync {
    pub fn new(home: impl Into<PathBuf>, manager: &str, player: &str) -> Self {
        let home = home.into();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .new_agent();
        let ledger = std::fs::read(home.join(".sync.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            home,
            manager: manager.trim_end_matches('/').to_string(),
            player: player.trim_end_matches('/').to_string(),
            agent,
            ledger,
            log: Vec::new(),
        }
    }

    /// What the last sync said, line by line.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    fn get_text(&self, url: &str) -> Result<String, String> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        response
            .body_mut()
            .with_config()
            .limit(256 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| format!("{url}: {e}"))
    }

    /// Fetch `url` into `relative` under the home unless the ledger says the
    /// file with that checksum is there already. Returns whether it was fetched.
    fn bring(&mut self, relative: &str, url: &str, sha256: &str) -> Result<bool, String> {
        let path = self.home.join(relative);
        let known = self.ledger.files.get(relative).cloned().unwrap_or_default();
        if !sha256.is_empty() && known == sha256 && path.is_file() {
            return Ok(false);
        }
        let bytes = self.get_bytes(url)?;
        if !sha256.is_empty() {
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if digest != sha256 {
                return Err(format!("{relative}: checksum {digest} is not {sha256}"));
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        let digest = if sha256.is_empty() {
            format!("{:x}", Sha256::digest(&bytes))
        } else {
            sha256.to_string()
        };
        self.ledger.files.insert(relative.to_string(), digest);
        Ok(true)
    }

    fn save_ledger(&self) -> Result<(), String> {
        let path = self.home.join(".sync.json");
        let text = serde_json::to_string_pretty(&self.ledger).unwrap_or_default();
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The configuration, the assets and the theme on show, brought up to
    /// date. The configuration files are rewritten to point into the home.
    pub fn run(&mut self) -> Result<Synced, String> {
        self.run_with(&Choice::default())
    }

    /// The same, with a theme and meter of the remote's own choosing where
    /// the choice says so.
    pub fn run_with(&mut self, choice: &Choice) -> Result<Synced, String> {
        self.log.clear();
        std::fs::create_dir_all(self.home.join("config"))
            .map_err(|e| format!("{}: {e}", self.home.display()))?;
        let config: RemoteConfig =
            serde_json::from_str(&self.get_text(&format!("{}/api/remote/config", self.manager))?)
                .map_err(|e| format!("remote config: {e}"))?;
        let texts = config_texts(&config, &self.home, choice);
        let theme_wanted = texts.theme.clone();
        std::fs::write(self.home.join("config/meter.txt"), &texts.meter)
            .map_err(|e| format!("meter.txt: {e}"))?;
        std::fs::write(self.home.join("config/spectrum.txt"), &texts.spectrum)
            .map_err(|e| format!("spectrum.txt: {e}"))?;
        let mut fetched = 0usize;
        let mut kept = 0usize;
        let mut count = |brought: bool| if brought { fetched += 1 } else { kept += 1 };

        for item in asset_plan(&config) {
            let url = format!("{}{}", self.manager, item.url);
            count(self.bring(&item.relative, &url, &item.sha256)?);
        }
        if config.assets.webfonts.is_empty() {
            self.log.push(
                "the player lists no web fonts (its Glass is older than 0.7.14): text is set in the plugin's font"
                    .to_string(),
            );
        }

        if let Some(local) = &choice.local {
            // The theme is on this machine: it only has to be there.
            if !local
                .templates
                .join(&theme_wanted)
                .join("meters.txt")
                .is_file()
            {
                return Err(format!(
                    "theme {theme_wanted} is not under {}",
                    local.templates.display()
                ));
            }
        } else {
            let theme: ThemeFiles = serde_json::from_str(&self.get_text(&format!(
                "{}/api/themes/{}/files",
                self.manager,
                encode(&theme_wanted)
            ))?)
            .map_err(|e| format!("theme files: {e}"))?;
            for item in theme_plan(&theme) {
                let url = format!("{}{}", self.manager, item.url);
                count(self.bring(&item.relative, &url, &item.sha256)?);
            }
        }
        self.ledger.version = config.version.clone();
        self.ledger.theme = theme_wanted.clone();
        self.save_ledger()?;
        Ok(Synced {
            version: config.version,
            theme: theme_wanted,
            meter: choice
                .meter
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or(config.meter),
            fetched,
            kept,
        })
    }
}

/// A stable id for this display: the host name, or a random one kept in the cache.
pub fn remote_id(cache: &Path) -> String {
    let file = cache.join("id");
    if let Ok(id) = std::fs::read_to_string(&file) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("remote-{:08x}", (nanos as u64) & 0xffff_ffff)
        });
    let _ = std::fs::create_dir_all(cache);
    let _ = std::fs::write(&file, &host);
    host
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tap::ring::Frame;
    #[test]
    fn an_uploaded_font_named_by_path_points_at_the_copy_brought_here() {
        let text = "[current]\nfont.path = /volumio/fonts\nfont.light = /Lato-Light.ttf\nfont.bold = /data/INTERNAL/glass/fonts/Mine.ttf\nfont.italic = builtin\n";
        let keys = custom_font_keys(
            text,
            Path::new("/home/x/customfonts"),
            &["Mine.ttf".to_string()],
        );
        assert_eq!(
            keys,
            vec![(
                "font.bold".to_string(),
                "/home/x/customfonts/Mine.ttf".to_string()
            )]
        );
        let none = custom_font_keys(text, Path::new("/home/x/customfonts"), &[]);
        assert!(
            none.is_empty(),
            "a font the player does not list is left as named"
        );
    }

    #[test]
    fn a_player_lists_its_web_fonts_and_every_icon_as_assets_with_checksums() {
        let listed = r#"{"version":"v","theme":"t","meter":"m","files":{"meter":"[current]\n"},
            "assets":{"fonts":[],"icons":[{"name":"mp3.svg","sha256":"ab"}],
            "webfonts":[{"name":"Lato-Light.ttf","sha256":"cd","bytes":3}]}}"#;
        let config: RemoteConfig = serde_json::from_str(listed).unwrap();
        assert_eq!(config.assets.webfonts[0].name, "Lato-Light.ttf");
        assert_eq!(config.assets.webfonts[0].sha256, "cd");
        assert_eq!(config.assets.icons[0].name, "mp3.svg");
        // A player older than 0.7.14 named its web fonts by file only, and
        // its web server answered a path to one with its page: not fetched.
        let older = r#"{"version":"v","theme":"t","files":{"meter":""},
            "assets":{"fonts":[],"icons":[]},"webfonts":["Lato-Light.ttf"]}"#;
        let config: RemoteConfig = serde_json::from_str(older).unwrap();
        assert!(config.assets.webfonts.is_empty());
    }

    #[test]
    fn the_hosts_addresses_are_read_from_a_routing_trie() {
        let trie = "Main:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 0.0.0.0\n        /0 universe UNICAST\n     +-- 127.0.0.0/8 2 0 2\n        +-- 127.0.0.0/31 1 0 0\n           |-- 127.0.0.0\n              /32 link BROADCAST\n              /8 host LOCAL\n           |-- 127.0.0.1\n              /32 host LOCAL\n     +-- 192.168.1.0/24 2 0 2\n        |-- 192.168.1.0\n           /32 link BROADCAST\n           /24 link UNICAST\n        |-- 192.168.1.10\n           /32 host LOCAL\n        |-- 192.168.1.255\n           /32 link BROADCAST\nLocal:\n  +-- 192.168.1.0/24 2 0 2\n        |-- 192.168.1.10\n           /32 host LOCAL\n";
        let found = local_addresses_in(trie);
        assert_eq!(
            found,
            vec!["192.168.1.10".parse::<std::net::Ipv4Addr>().unwrap()]
        );
    }

    #[test]
    fn frames_arrive_over_the_wire_and_merge_between_looks() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut hops = NetHops::new(server_addr, "t", "Test", "0.7.0").unwrap();
        // The first take subscribes; the server hears it and answers with two frames.
        let taken = hops.take();
        assert!(taken.hop.is_none() && taken.quiet);
        let mut buf = [0u8; 2048];
        let (n, client) = server.recv_from(&mut buf).unwrap();
        let sub: Value = serde_json::from_slice(&buf[..n]).unwrap();
        assert_eq!(sub["glass"], "subscribe");
        assert_eq!(sub["name"], "Test");
        let mut one = Frame {
            seq: 10,
            frames: 480,
            peak: [0.5, 0.2],
            rms: [0.3, 0.1],
            spectrum: [vec![0.5; 4], vec![0.5; 4]],
            ..Default::default()
        };
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        one.seq = 11;
        one.frames = 480;
        one.peak = [0.1, 0.9];
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        // A stale one (seq 9) and garbage are refused.
        one.seq = 9;
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        server.send_to(b"nonsense", client).unwrap();
        thread::sleep(Duration::from_millis(50));
        let taken = hops.take();
        let (frame, rate, elapsed) = taken.hop.expect("two frames merged");
        assert!(!taken.quiet);
        assert_eq!(rate, 48000);
        assert_eq!(elapsed, 960, "the frames of both hops");
        assert_eq!(frame.seq, 11);
        assert_eq!(frame.peak, [0.5, 0.9], "the highest of the two");
        assert_eq!(hops.stats(), (4, 1));
        thread::sleep(Duration::from_millis(600));
        assert!(hops.take().quiet, "no frame for half a second is silence");
    }

    #[test]
    fn a_new_stream_with_a_lower_count_but_a_later_stamp_is_taken() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut hops = NetHops::new(server_addr, "t", "Test", "0.7.6").unwrap();
        let _ = hops.take();
        let mut buf = [0u8; 2048];
        let (_, client) = server.recv_from(&mut buf).unwrap();
        let mut one = Frame {
            seq: 5000,
            time_ns: 10_000_000_000,
            frames: 480,
            peak: [0.5, 0.5],
            rms: [0.3, 0.3],
            spectrum: [vec![0.5; 4], vec![0.5; 4]],
            ..Frame::default()
        };
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        assert!(hops.take().hop.is_some());
        // The next stream's ring counts from one again; its stamps go on.
        one.seq = 1;
        one.time_ns += 20_000_000;
        one.peak = [0.9, 0.9];
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        let (frame, _, _) = hops.take().hop.expect("the new stream's packet");
        assert_eq!(frame.peak, [0.9, 0.9]);
        // A packet from before, arriving late, is still refused.
        one.seq = 4999;
        one.time_ns -= 40_000_000;
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        assert!(hops.take().hop.is_none(), "an old packet is not taken");
    }

    #[test]
    fn elapsed_frames_follow_the_players_stamps_and_silence_is_a_stop() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut hops = NetHops::new(server_addr, "t", "Test", "0.7.4").unwrap();
        let _ = hops.take();
        let mut buf = [0u8; 2048];
        let (_, client) = server.recv_from(&mut buf).unwrap();
        // A daemon of an earlier release put the ring's running count on the
        // wire, capped at 65535: the stamps say what really passed.
        let mut one = Frame {
            seq: 1,
            time_ns: 1_000_000_000,
            frames: 65535,
            peak: [0.5, 0.5],
            rms: [0.3, 0.3],
            spectrum: [vec![0.5; 4], vec![0.5; 4]],
            ..Frame::default()
        };
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        let (_, _, elapsed) = hops.take().hop.expect("the first packet");
        assert_eq!(elapsed, 65535, "the first packet has only its own count");
        one.seq = 2;
        one.time_ns += 20_000_000;
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        let (_, _, elapsed) = hops.take().hop.expect("the second packet");
        assert_eq!(
            elapsed, 960,
            "twenty milliseconds at 48 kHz, whatever the count says"
        );
        // Silence from the daemon: no hop, and quiet at once.
        let silence = Frame {
            seq: 3,
            time_ns: one.time_ns + 500_000_000,
            spectrum: [vec![0.0; 4], vec![0.0; 4]],
            ..Default::default()
        };
        server
            .send_to(&wire::encode(&silence, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        let taken = hops.take();
        assert!(taken.hop.is_none() && taken.quiet, "silence is a stop");
        // Sound again after the pause: the stamps are in order, so the gap
        // since the silence frame's stamp counts.
        one.seq = 4;
        one.time_ns += 1_000_000_000;
        server
            .send_to(&wire::encode(&one, 48000, 2, 0), client)
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        let taken = hops.take();
        let (_, _, elapsed) = taken.hop.expect("sound again");
        assert!(!taken.quiet);
        assert_eq!(
            elapsed, 24000,
            "half a second at 48 kHz since the silence frame"
        );
    }

    #[test]
    fn a_beacon_is_parsed_and_the_rest_ignored() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let beacon = Beacon::parse(
            br#"{"glass":"player","protocol":2,"name":"player","host":"player.local","frames_port":5580,"channel_port":5581,"manager_port":5582,"release":"0.7.0","config":"abcd1234","theme":"1280x720_x","meter":"random"}"#,
            ip,
        )
        .unwrap();
        assert_eq!(beacon.name, "player");
        assert_eq!(beacon.address(), "192.168.1.5");
        assert_eq!(beacon.manager_url(), "http://192.168.1.5:5582");
        assert!(
            Beacon::parse(br#"{"glass":"player","protocol":1}"#, ip).is_none(),
            "a player of the earlier wire is not for this remote"
        );
        assert!(Beacon::parse(br#"{"service":"peppy_level_server"}"#, ip).is_none());
        let bare = Beacon::parse(br#"{"glass":"player","protocol":2,"name":"x"}"#, ip).unwrap();
        assert_eq!(bare.frames_port, DEFAULT_FRAMES_PORT);
        assert_eq!(Beacon::named("player.local").address(), "player.local");
    }

    #[test]
    fn the_managers_status_names_the_ports_and_the_defaults_fill_the_rest() {
        let status: Value = serde_json::from_str(
            r#"{"ports":{"enabled":true,"frames":6580,"channel":6581,"beacon":6579,"manager":5582},
                "beacon":{"glass":"player","name":"player","release":"0.7.1","theme":"1280x720_x","meter":"random","player_port":3000}}"#,
        )
        .unwrap();
        let beacon = Beacon::from_status(&status, "player.local", 5582);
        assert_eq!(beacon.frames_port, 6580);
        assert_eq!(beacon.channel_port, 6581);
        assert_eq!(beacon.manager_port, 5582);
        assert_eq!(beacon.name, "player");
        assert_eq!(beacon.address(), "player.local");
        let bare = Beacon::from_status(&Value::Null, "10.0.0.5", 5590);
        assert_eq!(bare.frames_port, DEFAULT_FRAMES_PORT);
        assert_eq!(bare.manager_port, 5590);
        assert_eq!(bare.name, "10.0.0.5");
        let (unreachable, note) = Beacon::ask_manager("127.0.0.1", 1);
        assert!(note.is_some(), "a manager that does not answer is said");
        assert_eq!(unreachable.frames_port, DEFAULT_FRAMES_PORT);
        assert_eq!(unreachable.manager_port, 1);
    }

    #[test]
    fn a_configuration_is_pointed_into_the_home() {
        let text = "[current]\nmeter = gold\nbase.folder = /data/INTERNAL/glass/templates\nfont.path = /volumio/fonts\n\n[sdl.env]\nx = 1\n";
        let out = rewrite_config(
            text,
            &[
                ("base.folder", "/home/x/templates"),
                ("font.path", "/home/x/webfonts"),
                ("frame.rate", "30"),
            ],
        );
        assert!(out.contains("base.folder = /home/x/templates\n"));
        assert!(out.contains("font.path = /home/x/webfonts\n"));
        assert!(
            out.contains("[current]\nframe.rate = 30\n"),
            "a missing key lands under [current]: {out}"
        );
        assert!(out.contains("[sdl.env]\nx = 1\n"));
        assert_eq!(encode("1280x400_rose rs150"), "1280x400_rose%20rs150");
        assert_eq!(encode("a/b+c"), "a%2Fb%2Bc");
    }
}
