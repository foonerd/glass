//! A remote display's own affairs: its configuration, its state for its
//! page, and the page itself, served from a thread of its own on a port
//! of its own. The frame loop never waits on any of this: the page reads a
//! snapshot and a change lands as a new generation number the loop looks
//! at once a frame.

pub mod config;
pub mod run;

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use config::{Player, RemoteConfig, ThemeChoice};
use intake::remote::{Beacon, DEFAULT_BEACON_PORT};

const PAGE: &str = include_str!("page.html");

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
    /// This page's address on the network, once known.
    pub page: String,
}

/// The remote's state shared between the frame loop and the page.
pub struct RemoteApp {
    pub path: PathBuf,
    pub cache: PathBuf,
    config: Mutex<RemoteConfig>,
    generation: AtomicU64,
    status: Mutex<Status>,
}

impl RemoteApp {
    pub fn new(path: PathBuf, cache: PathBuf, config: RemoteConfig) -> Arc<Self> {
        Arc::new(Self {
            path,
            cache,
            config: Mutex::new(config),
            generation: AtomicU64::new(1),
            status: Mutex::new(Status::default()),
        })
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
        Ok(next)
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
                    "configPath": app.path.to_string_lossy(),
                    "cache": app.cache.to_string_lossy(),
                }),
            );
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
                    theme: ThemeChoice::Follow,
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
                    theme: ThemeChoice::Follow,
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
}
