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
use tap::ring::{merge_hops, Frame};
use tap::wire;

use crate::{Hops, Taken};

pub const DEFAULT_FRAMES_PORT: u16 = 5580;
pub const DEFAULT_CHANNEL_PORT: u16 = 5581;
pub const DEFAULT_MANAGER_PORT: u16 = 5582;
pub const DEFAULT_BEACON_PORT: u16 = 5579;
pub const DEFAULT_PLAYER_PORT: u16 = 3000;
/// A subscribe goes to the player this often; it forgets a remote after fifteen seconds.
const SUBSCRIBE_EVERY: Duration = Duration::from_secs(5);
/// No frame for this long is silence, as a ring gone quiet is.
const QUIET: Duration = Duration::from_millis(500);
const DATAGRAM_MAX: usize = 8192;

/// The frames as they arrive from the player, merged between two looks.
pub struct NetHops {
    socket: UdpSocket,
    server: SocketAddr,
    subscribe: Vec<u8>,
    subscribed_at: Option<Instant>,
    last_seq: Option<u32>,
    last_packet_at: Option<Instant>,
    rate: u32,
    buffer: Vec<u8>,
    /// Datagrams received and refused, for the log.
    pub received: u64,
    pub refused: u64,
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
            last_seq: None,
            last_packet_at: None,
            rate: 0,
            buffer: vec![0u8; DATAGRAM_MAX],
            received: 0,
            refused: 0,
        })
    }

    pub fn server(&self) -> SocketAddr {
        self.server
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
        let mut frames: Vec<Frame> = Vec::new();
        let mut elapsed: u64 = 0;
        loop {
            match self.socket.recv_from(&mut self.buffer) {
                Ok((n, from)) => {
                    if from.ip() != self.server.ip() {
                        continue;
                    }
                    self.received += 1;
                    match wire::decode(&self.buffer[..n]) {
                        Ok(packet) => {
                            let new = self
                                .last_seq
                                .is_none_or(|last| wire::Packet::is_after(packet.seq, last));
                            if !new {
                                continue;
                            }
                            self.last_seq = Some(packet.seq);
                            self.last_packet_at = Some(Instant::now());
                            self.rate = packet.rate;
                            elapsed += packet.frames as u64;
                            frames.push(packet.frame());
                        }
                        Err(_) => self.refused += 1,
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let quiet = self.last_packet_at.is_none_or(|at| at.elapsed() > QUIET);
        let hop =
            merge_hops(frames.into_iter()).map(|frame| (frame, self.rate.max(1), elapsed.max(1)));
        Taken { hop, quiet }
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

/// What a sync brought or confirmed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Synced {
    pub version: String,
    pub theme: String,
    pub meter: String,
    pub fetched: usize,
    pub kept: usize,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    sha256: String,
    #[serde(default)]
    bytes: u64,
}

#[derive(Deserialize)]
struct RemoteConfig {
    version: String,
    theme: String,
    #[serde(default)]
    meter: String,
    files: RemoteFiles,
    #[serde(default)]
    assets: RemoteAssets,
    #[serde(default)]
    webfonts: Vec<String>,
}

#[derive(Deserialize)]
struct RemoteFiles {
    meter: String,
    #[serde(default)]
    spectrum: String,
}

#[derive(Deserialize, Default)]
struct RemoteAssets {
    #[serde(default)]
    fonts: Vec<Asset>,
    #[serde(default)]
    icons: Vec<Asset>,
}

#[derive(Deserialize)]
struct ThemeFiles {
    folder: String,
    files: Vec<ThemeFile>,
    #[serde(default)]
    spectrum: Option<ThemeTree>,
}

#[derive(Deserialize)]
struct ThemeTree {
    folder: String,
    files: Vec<ThemeFile>,
}

#[derive(Deserialize)]
struct ThemeFile {
    path: String,
    sha256: String,
    #[serde(default)]
    bytes: u64,
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
        self.log.clear();
        std::fs::create_dir_all(self.home.join("config"))
            .map_err(|e| format!("{}: {e}", self.home.display()))?;
        let config: RemoteConfig =
            serde_json::from_str(&self.get_text(&format!("{}/api/remote/config", self.manager))?)
                .map_err(|e| format!("remote config: {e}"))?;
        let templates = self.home.join("templates");
        let spectrum_templates = self.home.join("templates_spectrum");
        let webfonts = self.home.join("webfonts");
        let meter_text = rewrite_config(
            &config.files.meter,
            &[
                ("base.folder", &templates.to_string_lossy()),
                ("font.path", &webfonts.to_string_lossy()),
            ],
        );
        let spectrum_text = rewrite_config(
            &config.files.spectrum,
            &[("base.folder", &spectrum_templates.to_string_lossy())],
        );
        std::fs::write(self.home.join("config/meter.txt"), meter_text)
            .map_err(|e| format!("meter.txt: {e}"))?;
        std::fs::write(self.home.join("config/spectrum.txt"), spectrum_text)
            .map_err(|e| format!("spectrum.txt: {e}"))?;
        let mut fetched = 0usize;
        let mut kept = 0usize;
        let mut count = |brought: bool| if brought { fetched += 1 } else { kept += 1 };

        for font in &config.assets.fonts {
            let url = format!(
                "{}/api/remote/asset/font/{}",
                self.manager,
                encode(&font.name)
            );
            count(self.bring(&format!("fonts/{}", font.name), &url, &font.sha256)?);
            let _ = font.bytes;
        }
        for icon in &config.assets.icons {
            let url = format!(
                "{}/api/remote/asset/icon/{}",
                self.manager,
                encode(&icon.name)
            );
            count(self.bring(&format!("format-icons/{}", icon.name), &url, &icon.sha256)?);
        }
        for name in &config.webfonts {
            let name = name.trim_start_matches('/');
            if name.is_empty() || name.contains("..") || name.contains('/') {
                continue;
            }
            let url = format!(
                "{}/app/themes/volumio3/assets/variants/volumio/fonts/{}",
                self.player,
                encode(name)
            );
            match self.bring(&format!("webfonts/{name}"), &url, "") {
                Ok(brought) => count(brought),
                Err(err) => self.log.push(format!("web font {name} not brought: {err}")),
            }
        }

        let theme: ThemeFiles = serde_json::from_str(&self.get_text(&format!(
            "{}/api/themes/{}/files",
            self.manager,
            encode(&config.theme)
        ))?)
        .map_err(|e| format!("theme files: {e}"))?;
        for file in &theme.files {
            let url = format!(
                "{}/api/themes/{}/file?tree=templates&path={}",
                self.manager,
                encode(&theme.folder),
                encode(&file.path)
            );
            count(self.bring(
                &format!("templates/{}/{}", theme.folder, file.path),
                &url,
                &file.sha256,
            )?);
            let _ = file.bytes;
        }
        if let Some(spectrum) = &theme.spectrum {
            for file in &spectrum.files {
                let url = format!(
                    "{}/api/themes/{}/file?tree=templates_spectrum&path={}",
                    self.manager,
                    encode(&spectrum.folder),
                    encode(&file.path)
                );
                count(self.bring(
                    &format!("templates_spectrum/{}/{}", spectrum.folder, file.path),
                    &url,
                    &file.sha256,
                )?);
            }
        }
        self.ledger.version = config.version.clone();
        self.ledger.theme = config.theme.clone();
        self.save_ledger()?;
        Ok(Synced {
            version: config.version,
            theme: config.theme,
            meter: config.meter,
            fetched,
            kept,
        })
    }
}

/// `key = value` lines at the top level of a configuration, the given keys
/// given new values; a key not there is added under `[current]`.
pub fn rewrite_config(text: &str, keys: &[(&str, &str)]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let mut replaced = None;
        for (key, value) in keys {
            if let Some(rest) = trimmed.strip_prefix(key) {
                if rest.trim_start().starts_with('=') {
                    replaced = Some(format!("{key} = {value}"));
                    seen.push(key);
                    break;
                }
            }
        }
        out.push(replaced.unwrap_or_else(|| line.to_string()));
    }
    for (key, value) in keys {
        if !seen.contains(key) {
            let at = out
                .iter()
                .position(|l| l.trim().eq_ignore_ascii_case("[current]"))
                .map(|i| i + 1)
                .unwrap_or(out.len());
            out.insert(at, format!("{key} = {value}"));
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Percent-encoding for a path segment or a query value.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
        assert_eq!(hops.refused, 1);
        assert_eq!(hops.received, 4);
        thread::sleep(Duration::from_millis(600));
        assert!(hops.take().quiet, "no frame for half a second is silence");
    }

    #[test]
    fn a_beacon_is_parsed_and_the_rest_ignored() {
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        let beacon = Beacon::parse(
            br#"{"glass":"player","protocol":1,"name":"hanger","host":"hanger.local","frames_port":5580,"channel_port":5581,"manager_port":5582,"release":"0.7.0","config":"abcd1234","theme":"1280x720_x","meter":"random"}"#,
            ip,
        )
        .unwrap();
        assert_eq!(beacon.name, "hanger");
        assert_eq!(beacon.address(), "192.168.1.5");
        assert_eq!(beacon.manager_url(), "http://192.168.1.5:5582");
        assert!(Beacon::parse(br#"{"glass":"player","protocol":2}"#, ip).is_none());
        assert!(Beacon::parse(br#"{"service":"peppy_level_server"}"#, ip).is_none());
        let bare = Beacon::parse(br#"{"glass":"player","protocol":1,"name":"x"}"#, ip).unwrap();
        assert_eq!(bare.frames_port, DEFAULT_FRAMES_PORT);
        assert_eq!(Beacon::named("hanger.local").address(), "hanger.local");
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
