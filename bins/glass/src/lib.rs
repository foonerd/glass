//! The display: the player's own, a remote, or a review of a theme. The
//! `glass` binary and the Android shell both call [`run`].
//!
//! `--once` reads the pipes, plots, and rasters a single frame.
//! `--output frame.ppm` writes that frame. Without it, the process keeps the
//! latest scene in memory at about 30 steps a second.

use std::env;
use std::process::ExitCode;
use std::thread;
use std::time::Instant;

use controls::{controls_of, interactive_now, override_scene, Act, Pointer, Touch};
use expose::{fit_art, raster_over, write_png, Frame, MeterAssets, Motion, Pictures, Stack};
use intake::{Overrides, Selector, Source, TapSource};
use lead::{
    frame_period, should_mark_dismiss, Input, InteractiveMode, SkinDesc, DISMISS_FILE_VAR, RUN_FLAG,
};
use pane::{publish, write_ppm, PointerKind, Shown, Surface, WindowMode, WindowOptions};
use plot::{step, Scene};
use std::time::Duration;

mod governor;
mod remote;

use remote::run::RemoteSession;

/// What the command line asked of this run.
struct Run {
    once: bool,
    headless: bool,
    print_scene: bool,
    output: Option<String>,
    record: Option<String>,
    overrides: Overrides,
    snapshot: Option<String>,
    thumb: Option<u32>,
    settle_s: f32,
    threads: Option<usize>,
    /// Where a remote keeps what it brought from players.
    cache: Option<String>,
    manager_port: u16,
    /// A window at the theme's exact size, movable, for development.
    dev: bool,
}

/// The title of a development window: the theme, the meter and the size.
fn dev_title(theme: &str, meter: &str, width: u32, height: u32) -> String {
    format!("Glass dev: {theme} / {meter} {width}x{height}")
}

/// How a session ended: the program leaves, or a session starts again.
enum Outcome {
    Exit(ExitCode),
    Reload(&'static str),
}

/// Whether a fade may start now: the engine's lock file is older than the
/// fade plus a second, or absent. Touching it claims the fade.
fn fade_lock_free(duration_s: f32) -> bool {
    let lock = env::temp_dir().join("glass_fade_lock");
    let cooldown = Duration::from_secs_f32(duration_s.max(0.0) + 1.0);
    let free = match std::fs::metadata(&lock).and_then(|m| m.modified()) {
        Ok(modified) => modified.elapsed().map_or(true, |age| age > cooldown),
        Err(_) => true,
    };
    if free {
        let _ = std::fs::write(&lock, b"");
    }
    free
}

/// One recorded step, the form `plot` replays from `testdata/frames/`.
#[derive(serde::Serialize)]
struct Recorded<'a> {
    skin: &'a SkinDesc,
    input: &'a Input,
    scene: &'a Scene,
}

/// Run the display with `args` as a command line would give them, the
/// program name first; the process's exit code comes back. The `glass`
/// binary calls this with its own arguments, the Android shell with the
/// activity's.
pub fn run(args: Vec<String>) -> ExitCode {
    logline::init("glass");
    let mut once = false;
    let mut headless = false;
    let mut print_scene = false;
    let mut output: Option<String> = None;
    let mut record: Option<String> = None;
    let mut overrides = Overrides::default();
    let mut list = false;
    let mut snapshot: Option<String> = None;
    let mut thumb: Option<u32> = None;
    let mut settle_s: f32 = 4.0;
    let mut threads: Option<usize> = None;
    let mut remote: Option<String> = None;
    let mut remote_name: Option<String> = None;
    let mut cache: Option<String> = None;
    let mut config_file: Option<String> = None;
    let mut open_settings = false;
    let mut dev = false;
    let mut manager_port: u16 = intake::remote::DEFAULT_MANAGER_PORT;
    let mut args = args.into_iter().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--once" => once = true,
            "--headless" => headless = true,
            "--print" => print_scene = true,
            "--output" => match args.next() {
                Some(path) => output = Some(path),
                None => {
                    eprintln!("glass: --output needs a path");
                    return ExitCode::from(2);
                }
            },
            "--record" => match args.next() {
                Some(path) => record = Some(path),
                None => {
                    eprintln!("glass: --record needs a path");
                    return ExitCode::from(2);
                }
            },
            "--theme" => match args.next() {
                Some(theme) => overrides.theme = Some(theme),
                None => {
                    eprintln!("glass: --theme needs a folder name");
                    return ExitCode::from(2);
                }
            },
            "--meter" => match args.next() {
                Some(meter) => overrides.meter = Some(meter),
                None => {
                    eprintln!("glass: --meter needs a name, random, or a comma list");
                    return ExitCode::from(2);
                }
            },
            "--interval" => match args.next().and_then(|s| s.parse::<u32>().ok()) {
                Some(seconds) => overrides.interval = Some(seconds.max(1)),
                None => {
                    eprintln!("glass: --interval needs whole seconds");
                    return ExitCode::from(2);
                }
            },
            "--fps" => match args.next().and_then(|s| s.parse::<u32>().ok()) {
                Some(fps) => overrides.fps = Some(fps.clamp(1, 120)),
                None => {
                    eprintln!("glass: --fps needs a whole number");
                    return ExitCode::from(2);
                }
            },
            "--threads" => match args.next().and_then(|s| s.parse::<usize>().ok()) {
                Some(n) => threads = Some(n.clamp(1, 64)),
                None => {
                    eprintln!("glass: --threads needs a whole number");
                    return ExitCode::from(2);
                }
            },
            "--settle" => match args.next().and_then(|s| s.parse::<f32>().ok()) {
                Some(seconds) => settle_s = seconds.max(0.5),
                None => {
                    eprintln!("glass: --settle needs seconds");
                    return ExitCode::from(2);
                }
            },
            "--snapshot" => match args.next() {
                Some(dir) => snapshot = Some(dir),
                None => {
                    eprintln!("glass: --snapshot needs a directory");
                    return ExitCode::from(2);
                }
            },
            "--thumb" => match args.next().and_then(|s| s.parse::<u32>().ok()) {
                Some(width) => thumb = Some(width.clamp(16, 4096)),
                None => {
                    eprintln!("glass: --thumb needs a width in pixels");
                    return ExitCode::from(2);
                }
            },
            // A remote: of a player named here, found by discovery, or as its
            // configuration says when nothing follows.
            "--remote" => match args.peek() {
                Some(next) if !next.starts_with("--") => remote = args.next(),
                _ => remote = Some("auto".to_string()),
            },
            "--config" => match args.next() {
                Some(path) => config_file = Some(path),
                None => {
                    eprintln!("glass: --config needs a file");
                    return ExitCode::from(2);
                }
            },
            "--settings" => open_settings = true,
            "--dev" => dev = true,
            "--name" => match args.next() {
                Some(name) => remote_name = Some(name),
                None => {
                    eprintln!("glass: --name needs the name this display shows as");
                    return ExitCode::from(2);
                }
            },
            "--cache" => match args.next() {
                Some(dir) => cache = Some(dir),
                None => {
                    eprintln!("glass: --cache needs a directory");
                    return ExitCode::from(2);
                }
            },
            "--manager-port" => match args.next().and_then(|s| s.parse::<u16>().ok()) {
                Some(port) if port > 0 => manager_port = port,
                _ => {
                    eprintln!("glass: --manager-port needs a port number");
                    return ExitCode::from(2);
                }
            },
            "--list" => list = true,
            "--help" => {
                let ring = if cfg!(unix) {
                    "Reads the tap's ring under /dev/shm and the player's state."
                } else {
                    "Shows a player's meters as a remote display: glass --remote."
                };
                println!(
                    "glass [--once] [--headless] [--print] [--output frame.png|frame.ppm] [--record step.json]\n      \
                     [--theme FOLDER] [--meter NAME|random|a,b,c] [--interval SECONDS] [--fps N] [--threads N]\n      \
                     [--list] [--snapshot DIR [--settle SECONDS] [--thumb WIDTH]]\n      \
                     [--remote [HOST|discover] [--name NAME] [--cache DIR] [--config FILE] [--manager-port N] [--settings]] [--dev]\n\
                     {ring}\n\
                     A window opens when a screen is there (DISPLAY on Linux). --headless skips it.\n\
                     --output writes every frame as a PNG or PPM and still rasters.\n\
                     --record writes the skin, input and scene of each step as JSON.\n\
                     --theme, --meter, --interval and --fps stand in for the installed configuration's values.\n\
                     --threads N paints every frame on N threads; by default a frame takes from one thread up to one a core as it needs.\n\
                     --list prints the installed themes and their meters.\n\
                     --snapshot shows each meter of the theme (or of the --meter list) for --settle seconds\n\
                     and writes DIR/<theme>/<meter>.png, then leaves.\n\
                     --thumb writes DIR/<theme>/<meter>.thumb.png beside each snapshot, WIDTH pixels wide.\n\
                     --remote shows a player's meters from here: its configuration, theme, fonts and icons\n\
                     are brought into a home under --cache, the frames come over the network, and the\n\
                     display follows the player's theme or shows one of its own. HOST adds that player,\n\
                     discover listens for players, and nothing after --remote runs as the remote's\n\
                     configuration says. The remote's settings page is served on its own port; --config\n\
                     names the configuration file, and --settings opens the page in a browser (of a remote\n\
                     already running, or of the one this starts).\n\
                     --dev opens a window at the theme's exact size, with a title bar naming the theme and the\n\
                     meter on show, to move about a desktop while a theme is reviewed; nothing is saved."
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("glass: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    let run = Run {
        once,
        headless,
        print_scene,
        output,
        record,
        overrides,
        snapshot,
        thumb,
        settle_s,
        threads,
        cache,
        manager_port,
        dev,
    };
    if let Some(target) = remote {
        return remote::run::remote_main(
            run,
            &target,
            remote_name,
            config_file.as_deref(),
            open_settings,
        );
    }
    intake::set_overrides(run.overrides.clone());
    if list {
        // Written through a lock so a closed pipe (`| head`) ends the listing quietly.
        use std::io::Write;
        let out = std::io::stdout();
        let mut out = out.lock();
        for (theme, meters) in intake::installed_themes() {
            if writeln!(out, "{theme}").is_err() {
                return ExitCode::SUCCESS;
            }
            for meter in meters {
                if writeln!(out, "  {meter}").is_err() {
                    return ExitCode::SUCCESS;
                }
            }
        }
        return ExitCode::SUCCESS;
    }
    match session(&run, None, &mut None) {
        Outcome::Exit(code) => code,
        Outcome::Reload(_) => ExitCode::SUCCESS,
    }
}

/// One session of the display: a theme's meters shown from a source until
/// the window closes, the player says stop, or (on a remote) the theme or
/// the settings change and a session starts again. A remote's window is
/// handed in and kept between sessions.
fn session(
    run: &Run,
    mut remote: Option<&mut RemoteSession>,
    window: &mut Option<(WindowOptions, Surface)>,
) -> Outcome {
    let Run {
        once,
        headless,
        print_scene,
        output,
        record,
        overrides,
        snapshot,
        thumb,
        settle_s,
        threads,
        ..
    } = run;
    let (once, headless, print_scene, thumb, settle_s, threads) =
        (*once, *headless, *print_scene, *thumb, *settle_s, *threads);
    // A snapshot walks the meters in turn; the first stands in for the
    // configuration's meter so the rotation below stays still.
    let snapshot_names: Vec<String> = match (snapshot, &overrides.meter) {
        (Some(_), Some(meter)) if meter != "random" => meter
            .split(',')
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect(),
        (Some(_), _) => intake::installed_meter_names(),
        (None, _) => Vec::new(),
    };
    if snapshot.is_some() {
        match snapshot_names.first() {
            Some(first) => intake::set_overrides(Overrides {
                meter: Some(first.clone()),
                ..overrides.clone()
            }),
            None => {
                eprintln!("glass: --snapshot found no meters in the theme");
                return Outcome::Exit(ExitCode::from(2));
            }
        }
    }
    let mut snapshot_index = 0usize;
    let mut snapshot_last: Option<Frame> = None;
    let settle = Duration::from_secs_f32(settle_s);
    // A theme in random or list mode moves to its next meter on the timer,
    // or with the next title when the player says so.
    let mut selector = Selector::new(intake::installed_rotation());
    let mut skin = match selector.next() {
        Some(name) => intake::installed_skin_named(Some(&name)),
        None => intake::installed_skin(),
    };
    let mut source = match remote.as_deref_mut() {
        None => TapSource::installed().with_skin(&skin),
        Some(remote) => match remote.source(&skin) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("glass: {err}");
                return Outcome::Exit(ExitCode::from(1));
            }
        },
    };
    // The player's own display says which meter it shows, so remotes that
    // follow the player can show the same one; a remote says nothing.
    let reports = remote.is_none();
    let theme_folder = std::path::Path::new(&skin.theme_dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A remote showing the player's meter does not rotate on its own.
    let mirroring = remote.as_deref().is_some_and(|r| r.follow && r.same_meter);
    // Why a session starts again, when it does.
    let mut reload: Option<&'static str> = None;
    let frame_rate = intake::installed_frame_rate();
    let started = Instant::now();
    // GLASS_PROFILE prints where each frame's time goes, averaged over 60 frames, and what is kept in memory.
    let profiling = env::var_os("GLASS_PROFILE").is_some();
    // A frame is painted in row bands across the cores when it needs them:
    // from one thread up to one per core, at most eight. --threads fixes the count.
    let (threads, adaptive) = match threads {
        Some(fixed) => (fixed, None),
        None => (
            thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .min(8),
            Some(frame_rate),
        ),
    };
    // Painting time added for a bench, to see the painters and the governor
    // work on any machine.
    let bench_delay = env::var("GLASS_BENCH_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis);
    let mut motion = Motion::new(threads, adaptive);
    motion.bench_delay = bench_delay;
    let mut period = frame_period(frame_rate);
    // The governor lowers the rate when frames keep overrunning with the
    // painters at their most; not for a snapshot, a single frame, or a
    // rate asked for on the command line.
    let governing =
        !once && snapshot.is_none() && overrides.fps.is_none() && intake::installed_governor();
    let mut governor = governor::Governor::new(frame_rate, governing, started.elapsed());
    if reports {
        source.report_showing(&theme_folder, &skin.name, governor.target());
    }
    logline::say!(
        Info,
        "display",
        "frame.rate={frame_rate} size={}x{} theme={} meter={} threads={threads}{} governor={}",
        skin.width,
        skin.height,
        if skin.theme_dir.is_empty() {
            "none"
        } else {
            &skin.theme_dir
        },
        skin.name,
        if adaptive.is_some() { " as needed" } else { "" },
        if governing { "on" } else { "off" }
    );
    let mut assets = MeterAssets::load(&skin);
    logline::say!(
        Info,
        "display",
        "interactive controls {}",
        match (skin.run.interactive, interactive_now(&skin)) {
            (InteractiveMode::On, _) => "on for every theme",
            (InteractiveMode::Off, _) => "off for every theme",
            (InteractiveMode::Theme, true) => "on, as the theme says",
            (InteractiveMode::Theme, false) => "off, as the theme says",
        }
    );
    logline::say!(
        Verbose,
        "display",
        "fonts loaded {} of 5",
        assets.fonts.loaded()
    );
    if profiling {
        println!(
            "glass: memory kept for the meter: {}",
            memory_line(&assets.memory())
        );
    }
    // The engine fades the first frame in when the player asks for a start
    // animation, and every later meter in; a lock file shared with the
    // player's own engine keeps two starts within the fade's time from fading twice.
    let mut did_fade_in = false;
    let loaded_ms = started.elapsed().as_millis() as u64;
    if skin.transition.at_start
        && skin.transition.fade
        && fade_lock_free(skin.transition.duration_s)
    {
        motion.fade.begin_in(
            loaded_ms,
            skin.transition.duration_s,
            skin.transition.white,
            skin.transition.opacity,
        );
        did_fade_in = true;
    }
    motion.ramp.begin(loaded_ms);
    let mut switched_at = Instant::now();
    let mut last_title: Option<String> = None;
    let mut profile_sum: Vec<(&'static str, u64)> = Vec::new();
    let mut profile_loop = [0u64; 4];
    let mut profile_painted = 0u64;
    let mut profile_boxes = 0u64;
    let mut profile_frames = 0u32;
    let mut profile_window = Instant::now();
    // Every picture the scene names, decoded once per file and box.
    let mut pictures = Pictures::default();
    let show_window = screen_available() && !headless;
    let write_file = output.is_some() || snapshot.is_some();
    let serving_remote = false;
    // The window: the player's is full screen at the theme's size; a
    // remote's is as its settings say, and stays up between sessions when
    // those settings hold, following the theme's size.
    if show_window {
        let mut options = match remote.as_deref() {
            Some(remote) => remote.window_options(),
            None => WindowOptions {
                // `position.type = fit`: the theme scaled to the screen, its shape kept.
                fit: skin.run.fit,
                ..WindowOptions::default()
            },
        };
        if run.dev {
            options.mode = WindowMode::Windowed;
            options.fit = false;
            options.pointer = true;
            options.keys = true;
            options.title = dev_title(&theme_folder, &skin.name, skin.width, skin.height);
        }
        // The same window serves when only its title differs.
        let kept = matches!(
            window.as_ref(),
            Some((known, _)) if WindowOptions { title: options.title.clone(), ..known.clone() } == options
        );
        if kept {
            if let Some((known, surface)) = window.as_mut() {
                if known.title != options.title {
                    surface.set_title(&options.title);
                    known.title = options.title.clone();
                }
                if let Err(err) = surface.fit_to(skin.width, skin.height) {
                    eprintln!("glass: {err}");
                    return Outcome::Exit(ExitCode::from(1));
                }
            }
        } else {
            *window = None;
            match Surface::open_with(skin.width, skin.height, &options) {
                Ok(mut surface) => {
                    if remote.is_none() && !skin.run.centered {
                        surface.place_at(skin.run.x, skin.run.y);
                    }
                    if let Some(remote) = remote.as_deref() {
                        remote
                            .app
                            .set_status(|s| s.monitors = remote::run::monitors_of(&surface));
                    }
                    *window = Some((options, surface));
                }
                Err(err) => {
                    eprintln!("glass: {err}");
                    return Outcome::Exit(ExitCode::from(1));
                }
            }
        }
    } else {
        *window = None;
    }
    let mut surface: Option<&mut Surface> = window.as_mut().map(|(_, surface)| surface);
    // The run flag: the player stands it up while it runs, the plugin takes
    // it down to stop the player. A touch, when the theme wants one, leaves
    // the dismiss marker the launcher named so the plugin re-arms its timeout.
    let running_for_plugin = !once && show_window && remote.is_none();
    if running_for_plugin {
        let _ = std::fs::write(RUN_FLAG, b"");
        #[cfg(unix)]
        let _ = std::fs::set_permissions(
            RUN_FLAG,
            std::os::unix::fs::PermissionsExt::from_mode(0o777),
        );
    }
    let mut run_flag_checked = Instant::now();
    let mut leave: Option<&'static str> = None;
    // Move to another meter of the theme: its skin, pictures and motion start afresh.
    macro_rules! switch_meter {
        ($name:expr) => {{
            let name: String = $name;
            skin = intake::installed_skin_named(Some(&name));
            source.set_skin(&skin);
            assets = MeterAssets::load(&skin);
            pictures.clear();
            motion = Motion::new(threads, adaptive);
            motion.bench_delay = bench_delay;
            let now = started.elapsed().as_millis() as u64;
            if skin.transition.fade && fade_lock_free(skin.transition.duration_s) {
                motion.fade.begin_in(
                    now,
                    skin.transition.duration_s,
                    skin.transition.white,
                    skin.transition.opacity,
                );
                did_fade_in = true;
            }
            motion.ramp.begin(now);
            switched_at = Instant::now();
            logline::say!(Info, "display", "meter={name}");
            governor.reset(started.elapsed());
            period = frame_period(frame_rate);
            if reports {
                source.report_showing(&theme_folder, &skin.name, governor.target());
            }
            if run.dev {
                if let Some(s) = surface.as_mut() {
                    s.set_title(&dev_title(
                        &theme_folder,
                        &skin.name,
                        skin.width,
                        skin.height,
                    ));
                }
            }
        }};
    }

    // A step to another meter asked for by a tap; taken at the top of a frame.
    let mut meter_step: Option<i8> = None;
    // A bar being dragged: its value follows the finger until it lifts.
    let mut touch = Touch::new();
    loop {
        let frame_started = Instant::now();
        if let Some(step) = meter_step.take() {
            let name = if step > 0 {
                selector.next()
            } else {
                selector.previous()
            };
            if let Some(name) = name {
                switch_meter!(name);
            }
        }
        let input = source.poll();
        let polled_at = Instant::now();
        // On a remote: the player's theme changed and this one follows it,
        // or the settings changed on the page. The session starts again.
        if let Some(remote) = remote.as_deref_mut() {
            if let Some((version, theme, _)) = source.take_config() {
                let other_theme = !theme.is_empty() && theme != remote.theme;
                let other_version = !version.is_empty()
                    && !remote.config_version.is_empty()
                    && version != remote.config_version;
                if remote.follow && (other_theme || other_version) {
                    if other_theme {
                        logline::say!(Info, "remotes", "the player's theme is now {theme}");
                    } else {
                        logline::say!(
                            Info,
                            "remotes",
                            "the player's configuration is now {version}"
                        );
                    }
                    reload = Some("the player's configuration changed");
                    leave = Some("the player's configuration changed");
                }
            }
            if leave.is_none() && remote.settings_changed() {
                reload = Some("settings changed");
                leave = Some("settings changed");
            }
            remote.note(&source, &skin, governor.target());
            // The player moved to another meter of the theme: show it too.
            if mirroring {
                if let Some((theme, meter)) = source.take_showing() {
                    if theme == remote.theme
                        && meter != skin.name
                        && intake::installed_meter_names().contains(&meter)
                    {
                        switch_meter!(meter);
                    }
                }
            }
        }
        // A snapshot saves the settled frame of the meter on show and moves on.
        if let Some(dir) = snapshot {
            if switched_at.elapsed() >= settle {
                if let Some(frame) = &snapshot_last {
                    let theme = std::path::Path::new(&skin.theme_dir)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "theme".into());
                    let folder = std::path::Path::new(dir).join(theme);
                    let _ = std::fs::create_dir_all(&folder);
                    let stem = skin.name.replace('/', "_");
                    let file = folder.join(format!("{stem}.png"));
                    match write_png(&file, frame) {
                        Ok(()) => logline::say!(Info, "display", "snapshot {}", file.display()),
                        Err(err) => eprintln!("glass: snapshot {}: {err}", file.display()),
                    }
                    // The thumbnail keeps the frame's shape at the asked width.
                    if let Some(width) = thumb {
                        let height = (u64::from(frame.height) * u64::from(width)
                            / u64::from(frame.width.max(1)))
                        .max(1) as u32;
                        let small = fit_art(frame, width, height);
                        let file = folder.join(format!("{stem}.thumb.png"));
                        if let Err(err) = write_png(&file, &small) {
                            eprintln!("glass: thumbnail {}: {err}", file.display());
                        }
                    }
                }
                snapshot_index += 1;
                match snapshot_names.get(snapshot_index) {
                    Some(name) => {
                        let name = name.clone();
                        switch_meter!(name);
                    }
                    None => leave = Some("snapshots done"),
                }
            }
        } else if selector.rotates() && !mirroring {
            let due = if selector.on_title() {
                let title = &input.metadata.title;
                let changed = last_title.as_ref().is_some_and(|known| known != title);
                if last_title.as_ref() != Some(title) {
                    last_title = Some(title.clone());
                }
                changed
            } else {
                switched_at.elapsed() >= selector.interval()
            };
            if due {
                if let Some(name) = selector.next() {
                    switch_meter!(name);
                }
            }
        }
        let mut scene = step(&skin, &input);
        override_scene(&touch, &mut scene);
        let stepped_at = Instant::now();
        let mut rastered_at = stepped_at;
        if let Some(path) = record {
            let recorded = Recorded {
                skin: &skin,
                input: &input,
                scene: &scene,
            };
            let written = serde_json::to_string_pretty(&recorded)
                .map_err(|err| err.to_string())
                .and_then(|text| std::fs::write(path, text).map_err(|err| err.to_string()));
            if let Err(err) = written {
                eprintln!("glass: --record {path}: {err}");
                return Outcome::Exit(ExitCode::from(1));
            }
        }
        if print_scene {
            println!(
                "left {:.2} right {:.2} bars {}",
                scene.left,
                scene.right,
                scene.bars.len()
            );
        }
        if surface.is_some() || write_file {
            pictures.follow(&scene, &assets);
            if profiling {
                motion.profile = Some(Vec::new());
            }
            let painted = raster_over(
                &scene,
                Stack {
                    screen: None,
                    face: None,
                    front: assets.front.as_ref(),
                    needle: assets.indicator.as_ref(),
                    needle_right: assets.indicator_right.as_ref(),
                    face_at: skin.face_at,
                    fonts: Some(&assets.fonts),
                    art: pictures.art(),
                    icon: pictures.icon(),
                    spectra: &assets.spectra,
                    folder_pictures: pictures.folder_pictures(),
                    fanart: pictures.fanart(),
                    vinyl: pictures.vinyl(),
                    tonearm: assets.tonearm.as_ref(),
                    reels: pictures.reels(&scene, &assets),
                    indicators: assets.indicators.as_ref(),
                    base: Some(&assets.base),
                },
                &mut motion,
                started.elapsed().as_millis() as u64,
            );
            let frame = painted.frame;
            rastered_at = Instant::now();
            if let Some(window) = surface.as_mut() {
                if let Some(mode) = remote.as_deref().and_then(|r| r.app.take_window_request()) {
                    if let Err(err) = window.set_mode(mode) {
                        eprintln!("glass: window: {err}");
                    }
                }
                let shown = match window.show(frame, painted.damage) {
                    Ok(shown) => shown,
                    Err(err) => {
                        eprintln!("glass: {err}");
                        return Outcome::Exit(ExitCode::from(1));
                    }
                };
                // The pointer: a tap on one of the theme's controls acts, a
                // finger down on a bar drags its value until it lifts; any
                // other touch does what the touch rules say.
                let events = window.take_pointer();
                let mut acted = false;
                let mut dismiss = false;
                if !events.is_empty() && interactive_now(&skin) {
                    if let Some(indicators) = scene.indicators.as_ref() {
                        let controls = controls_of(indicators, assets.indicators.as_ref());
                        let margin = indicators.spec.touch_margin;
                        for event in events {
                            let pointer = Pointer {
                                kind: match event.kind {
                                    PointerKind::Down => controls::PointerKind::Down,
                                    PointerKind::Move => controls::PointerKind::Move,
                                    PointerKind::Up => controls::PointerKind::Up,
                                },
                                x: event.x,
                                y: event.y,
                            };
                            let reaction =
                                touch.pointer(pointer, &controls, margin, &input.metadata);
                            if pointer.kind == controls::PointerKind::Up {
                                acted = reaction.taken;
                            }
                            for act in reaction.acts {
                                match act {
                                    Act::Command(command) => {
                                        logline::say!(
                                            Verbose,
                                            "display",
                                            "{} {},{}: {} {}",
                                            if reaction.dragged {
                                                "drag to"
                                            } else {
                                                "tap at"
                                            },
                                            event.x,
                                            event.y,
                                            command.name,
                                            command
                                                .value
                                                .as_ref()
                                                .map(|v| v.to_string())
                                                .unwrap_or_default()
                                        );
                                        source.command(&command);
                                    }
                                    Act::MeterStep(step) => {
                                        meter_step = Some(step.clamp(-1, 1) as i8)
                                    }
                                    Act::Dismiss => dismiss = true,
                                }
                            }
                        }
                    }
                }
                match shown {
                    Shown::Kept => {}
                    Shown::Closed => leave = Some("window closed"),
                    Shown::LeaveFullscreen => {
                        if let Err(err) = window.set_mode(WindowMode::Windowed) {
                            eprintln!("glass: window: {err}");
                        }
                    }
                    Shown::EnterFullscreen => {
                        if let Err(err) = window.set_mode(WindowMode::Fullscreen) {
                            eprintln!("glass: window: {err}");
                        }
                    }
                    Shown::Touched => {
                        if acted {
                            // A control took the touch.
                        } else if remote.is_some() && !dismiss {
                            // A touch on a remote plays or pauses the player.
                            if let Some(remote) = remote.as_deref() {
                                remote.touched(&mut source);
                            }
                        } else if skin.run.exit_on_touch || dismiss {
                            let marker = env::var(DISMISS_FILE_VAR).ok();
                            if should_mark_dismiss(
                                marker.as_deref(),
                                false,
                                std::path::Path::new(RUN_FLAG).exists(),
                            ) {
                                let _ = std::fs::write(marker.as_deref().unwrap_or_default(), b"1");
                            }
                            leave = Some("touched");
                        }
                    }
                }
            }
            if let Some(path) = output {
                let written = if path.to_ascii_lowercase().ends_with(".png") {
                    write_png(std::path::Path::new(path), frame)
                } else {
                    write_ppm(path, frame).map_err(|e| e.to_string())
                };
                if let Err(err) = written {
                    eprintln!("glass: {err}");
                    return Outcome::Exit(ExitCode::from(1));
                }
            }
            if snapshot.is_some() {
                snapshot_last = Some(frame.clone());
            }
        }
        if serving_remote {
            publish(&scene);
        }
        if running_for_plugin
            && leave.is_none()
            && run_flag_checked.elapsed() >= Duration::from_millis(500)
        {
            run_flag_checked = Instant::now();
            if !std::path::Path::new(RUN_FLAG).exists() {
                leave = Some("run flag removed");
            }
        }
        if let Some(why) = leave {
            logline::say!(Info, "display", "leaving ({why})");
            // Leave the way the engine leaves: fade out when a fade in was shown.
            if let (Some(window), true) = (surface.as_mut(), did_fade_in && skin.transition.fade) {
                let now = started.elapsed().as_millis() as u64;
                motion.fade.begin_out(
                    now,
                    skin.transition.duration_s,
                    skin.transition.white,
                    skin.transition.opacity,
                );
                while motion.fade.running(started.elapsed().as_millis() as u64) {
                    let painted = raster_over(
                        &scene,
                        Stack {
                            screen: None,
                            face: None,
                            front: assets.front.as_ref(),
                            needle: assets.indicator.as_ref(),
                            needle_right: assets.indicator_right.as_ref(),
                            face_at: skin.face_at,
                            fonts: Some(&assets.fonts),
                            art: pictures.art(),
                            icon: pictures.icon(),
                            spectra: &assets.spectra,
                            folder_pictures: pictures.folder_pictures(),
                            fanart: pictures.fanart(),
                            vinyl: pictures.vinyl(),
                            tonearm: assets.tonearm.as_ref(),
                            reels: pictures.reels(&scene, &assets),
                            indicators: assets.indicators.as_ref(),
                            base: Some(&assets.base),
                        },
                        &mut motion,
                        started.elapsed().as_millis() as u64,
                    );
                    if window.show(painted.frame, painted.damage).is_err() {
                        break;
                    }
                    thread::sleep(period);
                }
            }
            break;
        }
        if profiling {
            if let Some(stages) = motion.profile.take() {
                if profile_sum.is_empty() {
                    profile_sum = stages.iter().map(|(n, _)| (*n, 0)).collect();
                }
                for (sum, (_, micros)) in profile_sum.iter_mut().zip(stages.iter()) {
                    sum.1 += micros;
                }
            }
            let shown_at = Instant::now();
            profile_painted += motion
                .damage()
                .iter()
                .map(|r| u64::from(r.w) * u64::from(r.h))
                .sum::<u64>();
            profile_boxes += motion.damage().len() as u64;
            profile_loop[0] += polled_at.duration_since(frame_started).as_micros() as u64;
            profile_loop[1] += stepped_at.duration_since(polled_at).as_micros() as u64;
            profile_loop[2] += rastered_at.duration_since(stepped_at).as_micros() as u64;
            profile_loop[3] += shown_at.duration_since(rastered_at).as_micros() as u64;
            profile_frames += 1;
            if profile_frames == 60 {
                let n = u64::from(profile_frames);
                let stages: Vec<String> = profile_sum
                    .iter()
                    .map(|(name, sum)| format!("{name} {}us", sum / n))
                    .collect();
                let achieved = n as f64 / profile_window.elapsed().as_secs_f64();
                profile_window = Instant::now();
                println!(
                    "glass: profile per frame: {achieved:.1} fps, painters {}, painted {}% in {:.1} boxes, poll {}us, step {}us, raster {}us [{}], show {}us",
                    motion.painters(), profile_painted * 100 / (n * u64::from(skin.width.max(1)) * u64::from(skin.height.max(1))), profile_boxes as f64 / n as f64, profile_loop[0] / n, profile_loop[1] / n, profile_loop[2] / n, stages.join(", "), profile_loop[3] / n
                );
                let moving: Vec<(&'static str, usize)> = motion
                    .memory()
                    .into_iter()
                    .chain(pictures.memory())
                    .collect();
                println!("glass: memory moving: {}", memory_line(&moving));
                profile_sum.clear();
                profile_loop = [0; 4];
                profile_painted = 0;
                profile_boxes = 0;
                profile_frames = 0;
            }
        }
        if once {
            break;
        }
        // The governor judges the frame against its period, a quarter past
        // it counting as overrun, so a window in step with the screen's
        // refresh is never taken for one; a change of rate is said, and
        // told to the plugin with the meter on show.
        let spent = frame_started.elapsed();
        match governor.frame(
            started.elapsed(),
            spent > period + period / 4,
            motion.painters() >= threads,
        ) {
            Some(governor::Change::Lowered(rate)) => {
                period = frame_period(rate);
                logline::say!(
                    Info,
                    "display",
                    "governor: {rate} fps, the theme is heavy for this player"
                );
                if reports {
                    source.report_showing(&theme_folder, &skin.name, rate);
                }
            }
            Some(governor::Change::Raised(rate)) => {
                period = frame_period(rate);
                logline::say!(Info, "display", "governor: back to {rate} fps");
                if reports {
                    source.report_showing(&theme_folder, &skin.name, rate);
                }
            }
            None => {}
        }
        // Pace to the frame rate: sleep what is left of the period, not a whole one.
        if spent < period {
            thread::sleep(period - spent);
        }
    }
    if running_for_plugin {
        let _ = std::fs::remove_file(RUN_FLAG);
    }
    match reload {
        Some(why) => Outcome::Reload(why),
        None => Outcome::Exit(ExitCode::SUCCESS),
    }
}

/// Whether a window can open: on Windows and macOS always, on Linux when
/// an X11 or Wayland display is named in the environment.
pub(crate) fn screen_available() -> bool {
    cfg!(any(windows, target_os = "macos", target_os = "android"))
        || env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
        || env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
}

/// Where a remote display keeps what it brought from players: `--cache`,
/// else the user's cache directory.
fn cache_dir(given: Option<&str>) -> std::path::PathBuf {
    if let Some(dir) = given {
        return std::path::PathBuf::from(dir);
    }
    if let Some(xdg) = env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(xdg).join("glass-remote");
    }
    if let Some(local) = env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(local).join("glass-remote");
    }
    if let Some(home) = env::var_os("HOME").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(home)
            .join(".cache")
            .join("glass-remote");
    }
    env::temp_dir().join("glass-remote")
}

/// Stores and their sizes in kB, with the total first.
fn memory_line(stores: &[(&'static str, usize)]) -> String {
    let total: usize = stores.iter().map(|(_, b)| b).sum();
    let parts: Vec<String> = stores
        .iter()
        .filter(|(_, b)| *b > 0)
        .map(|(name, b)| format!("{name} {}", b / 1024))
        .collect();
    format!("{} kB ({})", total / 1024, parts.join(", "))
}
