//! Running as a remote display: the player from the configuration, its
//! theme brought over, a session shown, and around again whenever the
//! settings, the player or the theme change.

use std::sync::Arc;
use std::time::{Duration, Instant};

use intake::remote::{frames_address, Beacon, Choice, NetHops, Sync, DEFAULT_BEACON_PORT};
use intake::{Overrides, RemoteHello, TapSource};
use lead::SkinDesc;
use pane::{Shown, Surface, WindowMode, WindowOptions};
use std::process::ExitCode;

use super::config::{Player, RemoteConfig, ThemeChoice, WindowMode as ConfiguredMode};
use super::RemoteApp;
use crate::{session, Outcome, Run};

/// How long a player that cannot be reached rests before the next try.
const RETRY: Duration = Duration::from_secs(5);
/// How often the status for the page is refreshed.
const STATUS_EVERY: Duration = Duration::from_secs(1);

/// What a session on a remote needs of its player.
pub struct RemoteSession {
    pub app: Arc<RemoteApp>,
    pub beacon: Beacon,
    /// The theme folder shown.
    pub theme: String,
    /// Whether the player's theme is followed.
    pub follow: bool,
    generation: u64,
    status_at: Instant,
    received_at_status: u64,
    name: String,
    page: String,
}

/// The window as the remote's display settings say, under a title.
fn window_options_of(display: &super::config::Display, title: String) -> WindowOptions {
    let mode = match display.mode {
        ConfiguredMode::Fullscreen => WindowMode::Fullscreen,
        ConfiguredMode::Windowed => WindowMode::Windowed,
        ConfiguredMode::Frameless => WindowMode::Frameless,
    };
    WindowOptions {
        mode,
        position: display.position,
        fit: display.fit,
        pointer: mode != WindowMode::Fullscreen,
        keys: true,
        title,
    }
}

impl RemoteSession {
    pub fn window_options(&self) -> WindowOptions {
        window_options_of(
            &self.app.config().display,
            format!("Glass Remote: {}", self.beacon.name),
        )
    }

    /// The source for a session: the frames over the wire, the channel over TCP.
    pub fn source(&self, skin: &SkinDesc) -> Result<TapSource, String> {
        let frames =
            frames_address(&self.beacon).map_err(|e| format!("{}: {e}", self.beacon.address()))?;
        let id = intake::remote::remote_id(&self.app.cache);
        let release = env!("CARGO_PKG_VERSION").to_string();
        let hops = NetHops::new(frames, &id, &self.name, &release)
            .map_err(|e| format!("frames socket: {e}"))?
            .with_gain_db(self.app.config().gain_db);
        let channel = intake::Channel::tcp(format!(
            "{}:{}",
            self.beacon.address(),
            self.beacon.channel_port
        ))
        .with_hello(RemoteHello {
            id,
            name: self.name.clone(),
            release,
            screen: [skin.width, skin.height],
            page: self.page.clone(),
        });
        println!(
            "glass: remote {} of {}: frames from {frames}, channel {}:{}",
            self.name,
            self.beacon.name,
            self.beacon.address(),
            self.beacon.channel_port
        );
        Ok(TapSource::installed()
            .with_hops(Box::new(hops))
            .with_channel(channel)
            .with_skin(skin))
    }

    /// Whether the settings changed since this session began.
    pub fn settings_changed(&self) -> bool {
        self.app.generation() != self.generation
    }

    /// Once a second, what the page shows about the session.
    pub fn note(&mut self, source: &TapSource, skin: &SkinDesc) {
        if self.status_at.elapsed() < STATUS_EVERY {
            return;
        }
        let elapsed = self.status_at.elapsed().as_secs_f32().max(0.001);
        self.status_at = Instant::now();
        let (received, refused) = source.hop_stats();
        let per_s = (received.saturating_sub(self.received_at_status)) as f32 / elapsed;
        self.received_at_status = received;
        let connected = source.channel_connected();
        let theme = self.theme.clone();
        let meter = skin.name.clone();
        let screen = [skin.width, skin.height];
        self.app.set_status(|s| {
            s.phase = if per_s > 0.0 {
                "showing".into()
            } else if connected {
                "showing, no frames: is the player playing?".into()
            } else {
                "waiting for the player".into()
            };
            s.connected = connected;
            s.frames_per_s = per_s;
            s.received = received;
            s.refused = refused;
            s.theme = theme;
            s.meter = meter;
            s.screen = screen;
        });
    }

    /// A touch on the window: play or pause the player.
    pub fn touched(&self, source: &mut TapSource) {
        source.command(&intake::Command::new("toggle"));
    }
}

/// A frame with a few lines of text: what the window shows when there is
/// nothing else to show.
fn status_frame(lines: &[String]) -> expose::Frame {
    let (width, height) = (800u32, 480u32);
    let mut frame = expose::Frame {
        width,
        height,
        rgba: vec![0; (width * height * 4) as usize],
    };
    for px in frame.rgba.as_chunks_mut::<4>().0 {
        *px = [18, 18, 18, 255];
    }
    let mut y = 120u32;
    for line in lines {
        expose::draw_text(&mut frame, 60, y, line);
        y += 28;
    }
    frame
}

/// Show `lines` until the window closes, the settings change, or `wait` passes.
fn status_screen(
    run: &Run,
    app: &Arc<RemoteApp>,
    window: &mut Option<(WindowOptions, Surface)>,
    lines: &[String],
    wait: Option<Duration>,
) -> Outcome {
    for line in lines {
        println!("glass: {line}");
    }
    let generation = app.generation();
    let started = Instant::now();
    let show_window = std::env::var_os("DISPLAY").is_some() && !run.headless;
    let frame = status_frame(lines);
    let whole = expose::Rect {
        x: 0,
        y: 0,
        w: frame.width,
        h: frame.height,
    };
    // The window as the settings say, so the session that follows keeps it.
    let options = window_options_of(&app.config().display, "Glass Remote".to_string());
    let same = matches!(window.as_ref(), Some((known, _)) if *known == options);
    if show_window && !same {
        *window = None;
        match Surface::open_with(frame.width, frame.height, &options) {
            Ok(surface) => *window = Some((options, surface)),
            Err(err) => {
                eprintln!("glass: {err}");
                return Outcome::Exit(ExitCode::from(1));
            }
        }
    }
    loop {
        if let Some((_, surface)) = window.as_mut() {
            let _ = surface.fit_to(frame.width, frame.height);
            match surface.show(&frame, &[whole]) {
                Ok(Shown::Closed) => return Outcome::Exit(ExitCode::SUCCESS),
                Ok(_) => {}
                Err(err) => {
                    eprintln!("glass: {err}");
                    return Outcome::Exit(ExitCode::from(1));
                }
            }
        }
        if app.generation() != generation {
            return Outcome::Reload("settings changed");
        }
        if wait.is_some_and(|w| started.elapsed() >= w) {
            return Outcome::Reload("trying again");
        }
        if run.once {
            return Outcome::Exit(ExitCode::SUCCESS);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The address a browser on the network reaches this remote's page at:
/// this host as the player sees it, or, with no player set yet, the
/// likeliest of this host's own addresses (a home network's 192.168 range
/// first, then 10, then 172.16, then whatever the default route has).
fn page_url(player_host: &str, player_port: u16, page_port: u16) -> String {
    let ip = if player_host.is_empty() {
        let own = intake::remote::own_addresses();
        let rank = |ip: &std::net::Ipv4Addr| match ip.octets() {
            [192, 168, ..] => 0,
            [10, ..] => 1,
            _ if ip.is_private() => 2,
            _ => 3,
        };
        own.iter()
            .min_by_key(|ip| rank(ip))
            .map(|ip| std::net::IpAddr::V4(*ip))
            .or_else(|| intake::remote::own_address_towards("192.0.2.1", 9))
    } else {
        intake::remote::own_address_towards(player_host, player_port)
    };
    let ip = ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "localhost".to_string());
    format!("http://{ip}:{page_port}/")
}

/// The other private addresses of this host, for the first-run screen,
/// when the one chosen might be on another network than the reader's.
fn other_addresses(chosen: &str) -> Vec<String> {
    intake::remote::own_addresses()
        .into_iter()
        .filter(|ip| ip.is_private() && !chosen.contains(&ip.to_string()))
        .map(|ip| ip.to_string())
        .take(3)
        .collect()
}

/// Whether a remote of this build already serves its page on `port` of
/// this host: its state answers with a release.
fn page_already_up(port: u16) -> Option<String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .new_agent();
    let mut answer = agent
        .get(&format!("http://127.0.0.1:{port}/api/state"))
        .call()
        .ok()?;
    let text = answer.body_mut().read_to_string().ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("release")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
}

/// Start `program` with `args` and let it run on its own. On Unix this
/// goes through `posix_spawnp` directly: the standard library's spawn
/// refers to glibc 2.39's `pidfd_spawnp`, and a binary linked against a
/// 2.39 sysroot then refuses to load on Volumio's glibc 2.36.
#[cfg(unix)]
fn start_detached(program: &str, args: &[&str]) -> Result<(), String> {
    use std::ffi::CString;
    let c = |text: &str| CString::new(text).map_err(|e| e.to_string());
    let program_c = c(program)?;
    let argv_c: Vec<CString> = std::iter::once(program)
        .chain(args.iter().copied())
        .map(c)
        .collect::<Result<_, _>>()?;
    let mut argv: Vec<*mut libc::c_char> = argv_c
        .iter()
        .map(|a| a.as_ptr() as *mut libc::c_char)
        .collect();
    argv.push(std::ptr::null_mut());
    let env_c: Vec<CString> = std::env::vars_os()
        .filter_map(|(k, v)| {
            let mut pair = k;
            pair.push("=");
            pair.push(v);
            CString::new(std::os::unix::ffi::OsStringExt::into_vec(pair)).ok()
        })
        .collect();
    let mut envp: Vec<*mut libc::c_char> = env_c
        .iter()
        .map(|e| e.as_ptr() as *mut libc::c_char)
        .collect();
    envp.push(std::ptr::null_mut());
    let mut pid: libc::pid_t = 0;
    // SAFETY: every pointer handed over lives until the call returns, each
    // array ends with a null, and posix_spawnp reads and does not keep them.
    let rc = unsafe {
        libc::posix_spawnp(
            &mut pid,
            program_c.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            argv.as_ptr(),
            envp.as_ptr(),
        )
    };
    if rc != 0 {
        return Err(std::io::Error::from_raw_os_error(rc).to_string());
    }
    // Reaped from a thread of its own, so it leaves no zombie behind.
    std::thread::spawn(move || {
        let mut status = 0;
        // SAFETY: waiting on a child this process started.
        unsafe { libc::waitpid(pid, &mut status, 0) };
    });
    Ok(())
}

#[cfg(not(unix))]
fn start_detached(program: &str, args: &[&str]) -> Result<(), String> {
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Open `url` in the user's browser, without waiting on it.
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let result = start_detached("cmd", &["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let result = start_detached("open", &[url]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = start_detached("xdg-open", &[url]);
    match result {
        Ok(()) => println!("glass: settings page opened in a browser: {url}"),
        Err(err) => {
            eprintln!("glass: could not open a browser ({err}); the settings page is {url}")
        }
    }
}

/// Run as a remote. `target` is a player's host, `discover`, or `auto` for
/// the configuration as it is. With `open_settings` the settings page opens
/// in a browser: of a remote already running here, or of this one.
pub fn remote_main(
    run: Run,
    target: &str,
    name: Option<String>,
    config_file: Option<&str>,
    open_settings: bool,
) -> ExitCode {
    let config_path = super::config::config_path(config_file);
    let cache_dir = crate::cache_dir(run.cache.as_deref());
    let (mut config, note) = RemoteConfig::load(&config_path);
    if let Some(note) = note {
        eprintln!("glass: {note}");
    }
    if let Some(name) = name {
        config.name = name;
    }
    match target {
        "auto" | "page" => {}
        "discover" => {
            println!("glass: listening for players on port {DEFAULT_BEACON_PORT} for 10 s");
            match intake::remote::discover(DEFAULT_BEACON_PORT, Duration::from_secs(10)) {
                Ok(list) if !list.is_empty() => {
                    for b in &list {
                        println!(
                            "glass: player {} at {} ({})",
                            b.name,
                            b.address(),
                            b.release
                        );
                    }
                    let first = &list[0];
                    config.put_player(Player {
                        host: if first.host.is_empty() {
                            first.address()
                        } else {
                            first.host.clone()
                        },
                        manager_port: first.manager_port,
                        name: first.name.clone(),
                        theme: ThemeChoice::Follow,
                    });
                }
                Ok(_) => println!("glass: no player announced itself; the page can add one"),
                Err(err) => eprintln!("glass: discovery on port {DEFAULT_BEACON_PORT}: {err}"),
            }
        }
        host => {
            let (beacon, note) = Beacon::ask_manager(host, run.manager_port);
            if let Some(note) = note {
                println!("glass: the player's manager did not answer: {note}");
            }
            config.put_player(Player {
                host: host.to_string(),
                manager_port: run.manager_port,
                name: beacon.name.clone(),
                theme: ThemeChoice::Follow,
            });
        }
    }
    config.tidy();
    if let Err(err) = config.check() {
        eprintln!("glass: configuration: {err}");
        return ExitCode::from(2);
    }
    if let Err(err) = config.save(&config_path) {
        eprintln!("glass: {err}");
    }
    let wanted_port = config.page_port;
    let app = RemoteApp::new(config_path.clone(), cache_dir.clone(), config);
    let page_port = match super::serve(app.clone()) {
        Ok(port) => {
            println!(
                "glass: settings page on port {port} ({})",
                config_path.display()
            );
            if open_settings {
                open_browser(&format!("http://127.0.0.1:{port}/"));
            }
            Some(port)
        }
        Err(err) => {
            // A remote already running here holds the port: this one is not
            // a second display, it is a way to that one's page.
            if let Some(release) = page_already_up(wanted_port) {
                let url = format!("http://127.0.0.1:{wanted_port}/");
                println!("glass: a remote (Glass {release}) is already running here; its settings page is {url}");
                if open_settings {
                    open_browser(&url);
                }
                return ExitCode::SUCCESS;
            }
            eprintln!("glass: {err}; the settings page is not served");
            None
        }
    };

    let mut window: Option<(WindowOptions, Surface)> = None;
    loop {
        let config = app.config();
        let generation = app.generation();
        let Some((_, player)) = config.player().map(|(id, p)| (id.to_string(), p.clone())) else {
            let url = page_port
                .map(|port| page_url("", 0, port))
                .unwrap_or_else(|| "the settings page".to_string());
            // Nothing of an earlier player's session stays on the page.
            app.set_status(|s| {
                *s = super::Status {
                    phase: "no player".to_string(),
                    page: url.clone(),
                    ..Default::default()
                }
            });
            let others = other_addresses(&url);
            let outcome = status_screen(
                &run,
                &app,
                &mut window,
                &[
                    "Glass Remote".to_string(),
                    String::new(),
                    "No player is set.".to_string(),
                    format!("Open {url} in a browser to add one."),
                    if others.is_empty() {
                        String::new()
                    } else {
                        format!("This machine is also {}.", others.join(", "))
                    },
                ],
                None,
            );
            match outcome {
                Outcome::Exit(code) => return code,
                Outcome::Reload(_) => continue,
            }
        };
        // The player: its manager says its ports and its name.
        app.set_phase("reaching the player");
        app.set_status(|s| {
            s.player = if player.name.is_empty() {
                player.host.clone()
            } else {
                player.name.clone()
            };
            s.host = player.host.clone();
        });
        let (beacon, unreached) = Beacon::ask_manager(&player.host, player.manager_port);
        let page = page_port
            .map(|port| page_url(&player.host, player.manager_port, port))
            .unwrap_or_default();
        app.set_status(|s| {
            s.player_release = beacon.release.clone();
            s.page = page.clone();
        });
        if let Some(note) = unreached {
            let outcome = status_screen(
                &run,
                &app,
                &mut window,
                &[
                    format!("Glass Remote: {}", player.host),
                    String::new(),
                    "The player cannot be reached.".to_string(),
                    note.clone(),
                    "Trying again in a moment.".to_string(),
                    if page.is_empty() {
                        String::new()
                    } else {
                        format!("Settings: {page}")
                    },
                ],
                Some(RETRY),
            );
            app.set_phase(&format!("player unreached: {note}"));
            match outcome {
                Outcome::Exit(code) => return code,
                Outcome::Reload(_) => continue,
            }
        }
        // The theme: the player's, or one of its own.
        let (choice, follow) = match &player.theme {
            ThemeChoice::Follow => (Choice::default(), true),
            ThemeChoice::Own { folder, meter } => (
                Choice {
                    theme: Some(folder.clone()),
                    meter: Some(meter.meter_value()),
                    interval_s: Some(meter.interval_s()),
                    on_title: Some(meter.on_title()),
                },
                false,
            ),
        };
        app.set_phase("syncing");
        let home = cache_dir.join(beacon.address().replace([':', '/'], "_"));
        let player_http = format!("http://{}:{}", beacon.address(), beacon.player_port);
        let mut sync = Sync::new(&home, &beacon.manager_url(), &player_http);
        let theme = match sync.run_with(&choice) {
            Ok(synced) => {
                println!(
                    "glass: synced from {}: theme {} ({} fetched, {} kept, configuration {})",
                    beacon.address(),
                    synced.theme,
                    synced.fetched,
                    synced.kept,
                    synced.version
                );
                app.set_status(|s| {
                    s.synced = format!(
                        "{} fetched, {} kept, configuration {}",
                        synced.fetched, synced.kept, synced.version
                    )
                });
                synced.theme
            }
            Err(err) => {
                eprintln!("glass: sync from {}: {err}", beacon.address());
                if !home.join("config/meter.txt").is_file() {
                    let outcome = status_screen(
                        &run,
                        &app,
                        &mut window,
                        &[
                            format!("Glass Remote: {}", beacon.name),
                            String::new(),
                            "The player's theme could not be brought over.".to_string(),
                            err.chars().take(90).collect(),
                            "Trying again in a moment.".to_string(),
                        ],
                        Some(RETRY),
                    );
                    app.set_phase(&format!("sync failed: {err}"));
                    match outcome {
                        Outcome::Exit(code) => return code,
                        Outcome::Reload(_) => continue,
                    }
                }
                eprintln!("glass: showing what was brought before");
                app.set_status(|s| {
                    s.synced = format!("kept from before; the last sync failed: {err}")
                });
                choice.theme.clone().unwrap_or_default()
            }
        };
        for line in sync.log() {
            println!("glass: {line}");
        }
        std::env::set_var(lead::HOME_VAR, &home);
        std::env::remove_var("GLASS_CONFIG");
        intake::set_player(&beacon.address(), beacon.player_port);
        intake::set_overrides(Overrides {
            fps: config.display.fps,
            ..run.overrides.clone()
        });
        let name = if config.name.trim().is_empty() {
            intake::remote::remote_id(&cache_dir)
        } else {
            config.name.clone()
        };
        let mut remote = RemoteSession {
            app: app.clone(),
            beacon: beacon.clone(),
            theme: theme.clone(),
            follow,
            generation,
            status_at: Instant::now(),
            received_at_status: 0,
            name,
            page,
        };
        if let ThemeChoice::Own { meter, .. } = &player.theme {
            println!("glass: own theme {theme}, meter {}", meter.meter_value());
        }
        match session(&run, Some(&mut remote), &mut window) {
            Outcome::Exit(code) => return code,
            Outcome::Reload(why) => {
                println!("glass: around again ({why})");
                app.set_phase(&format!("starting again: {why}"));
                continue;
            }
        }
    }
}
