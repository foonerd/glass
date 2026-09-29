//! The display's pipeline for a page in a browser: the same configuration,
//! theme, fonts and icons a remote display brings, the same hops the
//! remotes receive, the same scene and the same raster, compiled to
//! WebAssembly and driven by the page. The page fetches; this module reads
//! what the page put in the file table, steps and paints.
//!
//! The page's side of the contract, in the order things happen:
//!
//! 1. `configure` with the manager's `/api/remote/config` answer: the
//!    configuration is written into the table under the home, and the
//!    fonts and icons to fetch come back as a plan.
//! 2. `theme_files` with the manager's `/api/themes/<theme>/files` answer:
//!    the theme's files to fetch come back as a plan.
//! 3. `put_file` for every file of both plans, under its place in the home.
//! 4. `start` with the meter to show, or none for the configuration's.
//! 5. `hop` for every frames datagram and `event` for every line of the
//!    plugin's, as they come; `frame` at the page's rate.
//! 6. `pointer` for every finger or mouse event on the canvas, in the
//!    frame's pixels; what comes back is for the page to carry out: a
//!    command to the player, a meter stepped, a dismiss.
//!
//! The raw exports at the end wrap [`Face`] for a host without bindings.

use std::cell::RefCell;
use std::rc::Rc;

use controls::{controls_of, interactive_now, override_scene, Act, Pointer, Touch};
use expose::{raster_over, MeterAssets, Motion, Pictures, Stack};
use intake::bring::{
    asset_plan, config_texts, theme_plan, Bring, Choice, RemoteConfig, ThemeFiles,
};
use intake::hops::WireHops;
use intake::{decode_event, Event, Hops, Source, Taken, TapSource};
use lead::{vfs, Metadata, SkinDesc};
use plot::Indicators;

pub use controls::PointerKind;

/// Where the page's files live in the table.
pub const HOME: &str = "/glass";

/// The hops the page pushes, shared between the face and its source.
#[derive(Clone, Default)]
struct SharedHops(Rc<RefCell<WireHops>>);

impl Hops for SharedHops {
    fn take(&mut self) -> Taken {
        self.0.borrow_mut().take()
    }

    fn stats(&self) -> (u64, u64) {
        self.0.borrow().stats()
    }
}

/// One meter on show for one page.
struct Showing {
    skin: SkinDesc,
    assets: MeterAssets,
    source: TapSource,
    motion: Motion,
    /// The configuration's frame rate, for the page to pace its frames by.
    rate: u32,
    /// The last frame's indicators and metadata, for a finger on them.
    indicators: Option<Indicators>,
    metadata: Metadata,
    /// Every picture the scene names, decoded once per file and box.
    pictures: Pictures,
}

/// What a pointer event asked of the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Happened {
    /// A command for the player, for the manager to run.
    Command(intake::Command),
    /// The rotation stepped: the meter now on show.
    Meter(String),
    /// A dismiss button: what leaving the display means here.
    Dismiss,
}

/// The pipeline for a page.
#[derive(Default)]
pub struct Face {
    hops: SharedHops,
    theme: String,
    showing: Option<Showing>,
    /// Events that arrived before the meter was on show, kept for it.
    early: Vec<Event>,
    /// The player's last state, infinity and queue, given to every meter
    /// put on show, so a change of meter does not wait for the next push.
    last_state: Option<Event>,
    last_infinity: Option<Event>,
    last_queue: Option<Event>,
    /// The finger on the controls, kept between frames.
    touch: Touch,
    /// A wanted file was put or said to be missing since the last frame:
    /// the meter derives from the state again.
    landed: bool,
}

impl Face {
    pub fn new() -> Self {
        lead::set_home(HOME);
        intake::wants::set_host_pictures();
        Self::default()
    }

    fn in_home(relative: &str) -> String {
        format!("{HOME}/{}", relative.trim_start_matches('/'))
    }

    /// A file under its path in the home, as the plans and the wants name it.
    pub fn put_file(&mut self, relative: &str, bytes: Vec<u8>) {
        vfs::put(&Self::in_home(relative), bytes);
        self.landed = true;
    }

    /// The manager has no file for a wanted path: the meter stops asking.
    pub fn missing(&mut self, relative: &str) {
        vfs::mark_missing(&Self::in_home(relative));
        self.landed = true;
    }

    /// What the meter on show wants from the page: files to fetch and put,
    /// or to mark missing, and the artist's fanart set to answer. A want is
    /// listed again at the next frame until it is met.
    pub fn wants(&self) -> Vec<intake::wants::Want> {
        intake::wants::take()
    }

    /// The manager's answer to a fanart want, as its JSON.
    pub fn fanart_answer(&self, json: &str) {
        intake::wants::answer_fanart(json);
    }

    /// The manager's configuration answer: the two configuration files go
    /// into the table, pointed into the home; the fonts and icons to fetch
    /// come back.
    pub fn configure(&mut self, config_json: &str) -> Result<Vec<Bring>, String> {
        let config: RemoteConfig =
            serde_json::from_str(config_json).map_err(|e| format!("remote config: {e}"))?;
        let texts = config_texts(&config, std::path::Path::new(HOME), &Choice::default());
        self.theme = texts.theme.clone();
        self.put_file(lead::METER_CONFIG, texts.meter.into_bytes());
        self.put_file(lead::SPECTRUM_CONFIG, texts.spectrum.into_bytes());
        Ok(asset_plan(&config))
    }

    /// The manager's theme files answer: the files to fetch come back.
    pub fn theme_files(&self, files_json: &str) -> Result<Vec<Bring>, String> {
        let theme: ThemeFiles =
            serde_json::from_str(files_json).map_err(|e| format!("theme files: {e}"))?;
        Ok(theme_plan(&theme))
    }

    /// The theme the configuration names.
    pub fn theme(&self) -> &str {
        &self.theme
    }

    /// Put the meter on show, `None` for the configuration's own, reading
    /// the theme as the display reads it. The hops and the events already
    /// pushed carry over.
    pub fn start(&mut self, meter: Option<&str>) -> Result<(), String> {
        // A configuration that rotates names no meter of its own: the
        // first of the rotation stands in until the player says which.
        let name = meter.map(str::to_string).or_else(|| {
            let rotation = intake::installed_rotation();
            rotation.names.first().cloned()
        });
        let skin = intake::installed_skin_named(name.as_deref());
        if skin.theme_dir.is_empty() {
            return Err("the configuration names no theme".to_string());
        }
        if !lead::is_file(&std::path::Path::new(&skin.theme_dir).join("meters.txt")) {
            return Err(format!("no meters.txt under {}", skin.theme_dir));
        }
        let assets = MeterAssets::load(&skin);
        let bins = skin
            .spectra
            .iter()
            .map(|s| s.bins.max(1))
            .max()
            .unwrap_or(lead::DEFAULT_SPECTRUM_BINS);
        let mut source = TapSource::new(bins, skin.meter_max)
            .without_player()
            .with_hops(Box::new(self.hops.clone()))
            .with_skin(&skin);
        for event in self
            .last_state
            .iter()
            .chain(self.last_infinity.iter())
            .chain(self.last_queue.iter())
            .cloned()
            .chain(self.early.drain(..))
        {
            source.push_event(event);
        }
        let rate = intake::installed_frame_rate();
        self.showing = Some(Showing {
            skin,
            assets,
            source,
            motion: Motion::new(1, Some(rate)),
            rate,
            indicators: None,
            metadata: Metadata::default(),
            pictures: Pictures::default(),
        });
        self.touch = Touch::new();
        Ok(())
    }

    /// The meter before or after the one on show in the rotation, or in
    /// the theme's order without one; the name now on show.
    fn step_meter(&mut self, step: i32) -> Option<String> {
        let rotation = intake::installed_rotation();
        let names = if rotation.names.is_empty() {
            intake::installed_meter_names()
        } else {
            rotation.names
        };
        if names.is_empty() {
            return None;
        }
        let current = self.meter().to_string();
        let at = names.iter().position(|n| *n == current).unwrap_or(0) as i32;
        let next = (at + step).rem_euclid(names.len() as i32) as usize;
        let name = names[next].clone();
        self.start(Some(&name)).ok()?;
        Some(self.meter().to_string())
    }

    /// A finger or a mouse on the canvas, in the frame's pixels: a tap on
    /// a control, or a drag on a bar, as on the player's own screen; what
    /// it asked for comes back for the page to carry out.
    pub fn pointer(&mut self, event: Pointer) -> Vec<Happened> {
        let acts = {
            let Face { showing, touch, .. } = self;
            let Some(showing) = showing.as_mut() else {
                return Vec::new();
            };
            if !interactive_now(&showing.skin) {
                return Vec::new();
            }
            let Some(indicators) = showing.indicators.as_ref() else {
                return Vec::new();
            };
            let controls = controls_of(indicators, showing.assets.indicators.as_ref());
            touch
                .pointer(
                    event,
                    &controls,
                    indicators.spec.touch_margin,
                    &showing.metadata,
                )
                .acts
        };
        let mut out = Vec::new();
        for act in acts {
            match act {
                Act::Command(command) => out.push(Happened::Command(command)),
                Act::MeterStep(step) => {
                    if let Some(name) = self.step_meter(step) {
                        out.push(Happened::Meter(name));
                    }
                }
                Act::Dismiss => out.push(Happened::Dismiss),
            }
        }
        out
    }

    /// A frames datagram, as the daemon sends it.
    pub fn hop(&self, bytes: &[u8]) {
        self.hops.0.borrow_mut().push(bytes);
    }

    /// A line of the plugin's, as the channel carries it. Whether it was
    /// one the display understands.
    pub fn event(&mut self, line: &[u8]) -> bool {
        let Some(event) = decode_event(line) else {
            return false;
        };
        match &event {
            Event::State(_) => self.last_state = Some(event.clone()),
            Event::Infinity(_) => self.last_infinity = Some(event.clone()),
            Event::Queue(_) => self.last_queue = Some(event.clone()),
            _ => {}
        }
        match self.showing.as_mut() {
            Some(showing) => showing.source.push_event(event),
            None => self.early.push(event),
        }
        true
    }

    /// The meter on show.
    pub fn meter(&self) -> &str {
        self.showing.as_ref().map_or("", |s| s.skin.name.as_str())
    }

    /// Frames a second the configuration asks for, 30 before `start`.
    pub fn frame_rate(&self) -> u32 {
        self.showing.as_ref().map_or(30, |s| s.rate)
    }

    pub fn width(&self) -> u32 {
        self.showing.as_ref().map_or(0, |s| s.skin.width)
    }

    pub fn height(&self) -> u32 {
        self.showing.as_ref().map_or(0, |s| s.skin.height)
    }

    /// Step and paint one frame at `now_ms` on the page's clock; the RGBA
    /// bytes stay as they are until the next frame.
    pub fn frame(&mut self, now_ms: u64) -> Option<&expose::Frame> {
        #[cfg(target_arch = "wasm32")]
        lead::set_clock_us(now_ms.saturating_mul(1000));
        let Face {
            showing,
            touch,
            landed,
            ..
        } = self;
        let showing = showing.as_mut()?;
        if std::mem::take(landed) {
            showing.source.refresh();
        }
        let input = showing.source.poll();
        let mut scene = plot::step(&showing.skin, &input);
        override_scene(touch, &mut scene);
        showing.indicators = scene.indicators.clone();
        showing.metadata = input.metadata;
        showing.pictures.follow(&scene, &showing.assets);
        let assets = &showing.assets;
        let stack = Stack {
            screen: None,
            face: None,
            front: assets.front.as_ref(),
            needle: assets.indicator.as_ref(),
            needle_right: assets.indicator_right.as_ref(),
            face_at: showing.skin.face_at,
            fonts: Some(&assets.fonts),
            art: showing.pictures.art(),
            icon: showing.pictures.icon(),
            spectra: &assets.spectra,
            folder_pictures: showing.pictures.folder_pictures(),
            fanart: showing.pictures.fanart(),
            vinyl: showing.pictures.vinyl(),
            tonearm: assets.tonearm.as_ref(),
            reels: showing.pictures.reels(&scene, assets),
            indicators: assets.indicators.as_ref(),
            base: Some(&assets.base),
        };
        let painted = raster_over(&scene, stack, &mut showing.motion, now_ms);
        Some(painted.frame)
    }
}

/// The raw exports, for the browser only: on a machine the names would
/// shadow the C library's (`free` first of all) in whatever links this crate.
#[cfg(target_arch = "wasm32")]
mod exports {
    use super::*;

    // ---- the raw exports ------------------------------------------------------
    //
    // A host without a bindings layer: bytes go in through `alloc`ed buffers,
    // answers come back as a string the host reads at `answer_ptr` for
    // `answer_len` bytes until the next call that answers.

    thread_local! {
        static FACE: RefCell<Face> = RefCell::new(Face::new());
        static ANSWER: RefCell<String> = const { RefCell::new(String::new()) };
    }

    unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
        if len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(ptr, len)
        }
    }

    /// A panic's message becomes the answer, so a page that catches the
    /// trap can say why; set once, at the first call in.
    fn armed() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                let text = format!("{info}");
                ANSWER.with(|slot| *slot.borrow_mut() = text);
            }));
        });
    }

    fn answer(text: String) {
        ANSWER.with(|slot| *slot.borrow_mut() = text);
    }

    fn plan_json(plan: &[Bring]) -> String {
        let items: Vec<serde_json::Value> = plan
            .iter()
            .map(|b| serde_json::json!({ "path": b.relative, "url": b.url, "sha256": b.sha256 }))
            .collect();
        serde_json::Value::Array(items).to_string()
    }

    /// `len` bytes the host may write into; freed with `free`.
    #[no_mangle]
    pub extern "C" fn alloc(len: usize) -> *mut u8 {
        let mut bytes: Vec<u8> = Vec::with_capacity(len.max(1));
        let ptr = bytes.as_mut_ptr();
        std::mem::forget(bytes);
        ptr
    }

    /// # Safety
    /// `ptr` and `len` came from `alloc`.
    #[no_mangle]
    pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }

    #[no_mangle]
    pub extern "C" fn answer_ptr() -> *const u8 {
        ANSWER.with(|slot| slot.borrow().as_ptr())
    }

    #[no_mangle]
    pub extern "C" fn answer_len() -> usize {
        ANSWER.with(|slot| slot.borrow().len())
    }

    /// A file of the home, by its path in a plan.
    /// # Safety
    /// The pointers name `alloc`ed buffers of the given lengths.
    #[no_mangle]
    pub unsafe extern "C" fn put_file(
        path: *const u8,
        path_len: usize,
        data: *const u8,
        data_len: usize,
    ) {
        armed();
        let path = String::from_utf8_lossy(slice(path, path_len)).into_owned();
        let bytes = slice(data, data_len).to_vec();
        FACE.with(|face| face.borrow_mut().put_file(&path, bytes));
    }

    /// A wanted file the manager has none of, by its path in the want.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn missing(path: *const u8, path_len: usize) {
        armed();
        let path = String::from_utf8_lossy(slice(path, path_len)).into_owned();
        FACE.with(|face| face.borrow_mut().missing(&path));
    }

    /// What the meter wants from the page; the answer is the list as JSON:
    /// `{"kind":"file","path":…,"url":…}` to fetch and `put_file` or mark
    /// `missing`, `{"kind":"fanart","artist":…,"uri":…}` to answer with
    /// `fanart_answer`.
    #[no_mangle]
    pub extern "C" fn wants() -> u32 {
        armed();
        let list = FACE.with(|face| face.borrow().wants());
        answer(serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()));
        0
    }

    /// The manager's fanart answer for the artist wanted, as its JSON.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn fanart_answer(json: *const u8, len: usize) {
        armed();
        let text = String::from_utf8_lossy(slice(json, len)).into_owned();
        FACE.with(|face| face.borrow().fanart_answer(&text));
    }

    /// The configuration answer; the answer is the plan as JSON, or 1 with
    /// the error as the answer.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn configure(json: *const u8, len: usize) -> i32 {
        armed();
        let text = String::from_utf8_lossy(slice(json, len)).into_owned();
        match FACE.with(|face| face.borrow_mut().configure(&text)) {
            Ok(plan) => {
                answer(plan_json(&plan));
                0
            }
            Err(e) => {
                answer(e);
                1
            }
        }
    }

    /// The theme files answer; the answer is the plan as JSON, or 1 with the error.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn theme_files(json: *const u8, len: usize) -> i32 {
        armed();
        let text = String::from_utf8_lossy(slice(json, len)).into_owned();
        match FACE.with(|face| face.borrow().theme_files(&text)) {
            Ok(plan) => {
                answer(plan_json(&plan));
                0
            }
            Err(e) => {
                answer(e);
                1
            }
        }
    }

    /// Put the meter on show, an empty name for the configuration's; 1 with
    /// the error as the answer when the theme cannot be read.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn start(meter: *const u8, len: usize) -> i32 {
        armed();
        let name = String::from_utf8_lossy(slice(meter, len)).into_owned();
        let meter = if name.trim().is_empty() {
            None
        } else {
            Some(name.as_str())
        };
        match FACE.with(|face| face.borrow_mut().start(meter)) {
            Ok(()) => {
                answer(FACE.with(|face| face.borrow().meter().to_string()));
                0
            }
            Err(e) => {
                answer(e);
                1
            }
        }
    }

    /// A frames datagram.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn hop(bytes: *const u8, len: usize) {
        armed();
        let bytes = slice(bytes, len);
        FACE.with(|face| face.borrow().hop(bytes));
    }

    /// A line of the plugin's; 1 when it was not one the display understands.
    /// # Safety
    /// The pointer names an `alloc`ed buffer of the given length.
    #[no_mangle]
    pub unsafe extern "C" fn event(line: *const u8, len: usize) -> i32 {
        armed();
        let line = slice(line, len);
        if FACE.with(|face| face.borrow_mut().event(line)) {
            0
        } else {
            1
        }
    }

    /// The frame at `now_ms`: a pointer to `frame_width` by `frame_height`
    /// RGBA bytes, null before `start`.
    #[no_mangle]
    pub extern "C" fn frame(now_ms: u64) -> *const u8 {
        armed();
        FACE.with(|face| {
            face.borrow_mut()
                .frame(now_ms)
                .map_or(std::ptr::null(), |frame| frame.rgba.as_ptr())
        })
    }

    /// A pointer event in the frame's pixels, `kind` 0 down, 1 move, 2 up;
    /// the answer is what it asked for as JSON: `{"command":{"name","value"}}`,
    /// `{"meter":"<name>"}` or `{"dismiss":true}` each, in an array.
    #[no_mangle]
    pub extern "C" fn pointer(kind: u32, x: i32, y: i32) -> i32 {
        armed();
        let kind = match kind {
            0 => PointerKind::Down,
            1 => PointerKind::Move,
            _ => PointerKind::Up,
        };
        let happened = FACE.with(|face| face.borrow_mut().pointer(Pointer { kind, x, y }));
        let items: Vec<serde_json::Value> = happened
            .iter()
            .map(|h| match h {
                Happened::Command(c) => {
                    serde_json::json!({ "command": { "name": c.name, "value": c.value } })
                }
                Happened::Meter(name) => serde_json::json!({ "meter": name }),
                Happened::Dismiss => serde_json::json!({ "dismiss": true }),
            })
            .collect();
        answer(serde_json::Value::Array(items).to_string());
        happened.len() as i32
    }

    #[no_mangle]
    pub extern "C" fn frame_rate() -> u32 {
        FACE.with(|face| face.borrow().frame_rate())
    }

    #[no_mangle]
    pub extern "C" fn frame_width() -> u32 {
        FACE.with(|face| face.borrow().width())
    }

    #[no_mangle]
    pub extern "C" fn frame_height() -> u32 {
        FACE.with(|face| face.borrow().height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METER_TXT: &str = "[current]\nbase.folder = /data/INTERNAL/glass/templates\nmeter.folder = 480x320\nmeter = random\nfont.path = /volumio/fonts\nframe.rate = 30\n";

    fn config_json() -> String {
        serde_json::json!({
            "version": "v1", "theme": "480x320", "meter": "random",
            "files": { "meter": METER_TXT, "spectrum": "[current]\nbase.folder = /x\nspectrum.folder = 480x320\n" },
            "assets": { "fonts": [{ "name": "DSEG7Classic-Italic.ttf", "sha256": "a" }], "icons": [], "webfonts": [], "custom": [] }
        })
        .to_string()
    }

    fn repo_theme(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugin/templates")
            .join(name)
    }

    /// The table under the home is one for the process: the tests that
    /// fill it take turns.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The repository's theme into the table; its `meters.txt` comes back
    /// for a test to extend.
    fn theme_into_table(face: &mut Face, name: &str) -> String {
        let mut meters = String::new();
        for entry in std::fs::read_dir(repo_theme(name)).expect("the test theme") {
            let path = entry.expect("an entry").path();
            if path.is_file() {
                let file = path.file_name().unwrap().to_string_lossy().into_owned();
                let bytes = std::fs::read(&path).unwrap();
                if file == "meters.txt" {
                    meters = String::from_utf8_lossy(&bytes).into_owned();
                }
                face.put_file(&format!("templates/{name}/{file}"), bytes);
            }
        }
        meters
    }

    #[test]
    fn the_plans_place_every_file_under_the_home() {
        let mut face = Face::new();
        let plan = face.configure(&config_json()).expect("a plan");
        assert_eq!(plan[0].relative, "fonts/DSEG7Classic-Italic.ttf");
        assert_eq!(face.theme(), "480x320");
        assert!(
            lead::read_to_string(std::path::Path::new("/glass/config/meter.txt"))
                .expect("the configuration in the table")
                .contains("base.folder = /glass/templates\n")
        );
        let files = serde_json::json!({ "folder": "480x320", "files": [{ "path": "meters.txt", "sha256": "" }] }).to_string();
        let plan = face.theme_files(&files).expect("a plan");
        assert_eq!(plan[0].relative, "templates/480x320/meters.txt");
    }

    #[test]
    fn a_theme_from_the_table_paints_a_frame() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        theme_into_table(&mut face, "480x320");
        face.start(None).expect("the meter on show");
        assert_eq!((face.width(), face.height()), (480, 320));
        assert!(!face.meter().is_empty());
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","artist":"Someone"}}"#
        ));
        assert!(!face.event(b"{\"kind\":\"other\"}"));
        let frame = face.frame(40).expect("a frame");
        assert_eq!(frame.rgba.len(), 480 * 320 * 4);
        assert!(frame.rgba.iter().any(|b| *b != 0), "something was painted");
    }

    /// A meter given a volume bar and the interactive word: a finger down
    /// and up along the bar sends the volume where it lifted; the state
    /// pushed before is still the meter's after a step to another meter.
    #[test]
    fn a_finger_on_a_bar_asks_for_the_volume_and_the_state_survives_a_step() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        let meters = theme_into_table(&mut face, "480x320");
        let bar_at = meters.find("[bar]").expect("the bar meter");
        let extended = "[bar]\nconfig.extend = True\ninteractive = True\nvolume.pos = 40,300\nvolume.dim = 200,4\nvolume.style = slider\n";
        let meters = format!("{}{}{}", &meters[..bar_at], extended, &meters[bar_at + 5..]);
        face.put_file("templates/480x320/meters.txt", meters.into_bytes());
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","artist":"Someone","volume":30}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        assert_eq!(face.meter(), "bar");
        face.frame(40).expect("a frame");
        let down = face.pointer(Pointer {
            kind: PointerKind::Down,
            x: 60,
            y: 301,
        });
        assert!(down.is_empty());
        let up = face.pointer(Pointer {
            kind: PointerKind::Up,
            x: 140,
            y: 301,
        });
        assert_eq!(
            up,
            vec![Happened::Command(intake::Command::with(
                "volume",
                serde_json::json!(50)
            ))]
        );
        // Nothing under the finger: nothing asked.
        let nothing = face.pointer(Pointer {
            kind: PointerKind::Up,
            x: 400,
            y: 30,
        });
        assert!(nothing.is_empty());
        // Another meter of the theme keeps the player's state.
        let stepped = face.step_meter(1).expect("a next meter");
        assert_ne!(stepped, "bar");
        face.frame(80).expect("a frame");
        assert_eq!(
            face.showing.as_ref().map(|s| s.metadata.title.as_str()),
            Some("A song")
        );
    }

    /// A meter with next-track rows: the track after the playing one comes
    /// from the queue the plugin pushes, a queue pushed later moves it under
    /// the same track, a queue pushed before the meter is on show is kept
    /// for it, and the last track has nothing after it.
    #[test]
    fn the_next_track_comes_from_the_queue_the_plugin_pushes() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        let meters = theme_into_table(&mut face, "480x320");
        let bar_at = meters.find("[bar]").expect("the bar meter");
        let extended = "[bar]\nconfig.extend = True\nplayinfo.next.title.pos = 10,300\nplayinfo.next.artist.pos = 10,280\n";
        let meters = format!("{}{}{}", &meters[..bar_at], extended, &meters[bar_at + 5..]);
        face.put_file("templates/480x320/meters.txt", meters.into_bytes());
        assert!(face.event(
            br#"{"kind":"queue","items":[{"name":"First","artist":"A","duration":100},{"name":"Second","artist":"B","album":"Two","duration":200}]}"#
        ));
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"First","artist":"A","position":0}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        face.frame(40).expect("a frame");
        let next = |face: &Face| {
            face.showing.as_ref().map(|s| {
                (
                    s.metadata.next_title.clone(),
                    s.metadata.next_artist.clone(),
                )
            })
        };
        assert_eq!(next(&face), Some(("Second".into(), "B".into())));
        assert!(face.event(
            br#"{"kind":"queue","items":[{"name":"First","artist":"A"},{"name":"Third","artist":"C"}]}"#
        ));
        face.frame(80).expect("a frame");
        assert_eq!(next(&face), Some(("Third".into(), "C".into())));
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"Third","artist":"C","position":1}}"#
        ));
        face.frame(120).expect("a frame");
        assert_eq!(next(&face), Some((String::new(), String::new())));
    }

    /// Queue progress mode on the Face: the seconds before the playing
    /// track and the whole queue's length come from the queue the plugin
    /// pushes, so the bar runs over the queue as on the player's screen.
    #[test]
    fn queue_progress_comes_from_the_queue_the_plugin_pushes() {
        let _serial = serial();
        let mut face = Face::new();
        let config = config_json().replace(
            "frame.rate = 30\\n",
            "frame.rate = 30\\nqueue.mode = queue\\n",
        );
        assert_ne!(
            config,
            config_json(),
            "the queue mode is in the configuration"
        );
        face.configure(&config).expect("a plan");
        theme_into_table(&mut face, "480x320");
        assert!(face.event(
            br#"{"kind":"queue","items":[{"name":"First","duration":100},{"name":"Second","duration":200},{"name":"Third","duration":50}]}"#
        ));
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"Second","position":1,"duration":200,"seek":10000}}"#
        ));
        face.start(None).expect("the meter on show");
        face.frame(40).expect("a frame");
        let meta = &face.showing.as_ref().unwrap().metadata;
        assert_eq!((meta.queue_before_s, meta.queue_total_s), (100.0, 350.0));
    }

    /// A meter given a type area: the track's type names an icon the page
    /// brought under the home; the icon is found there and painted in the
    /// area's colour, rather than the label standing in for it.
    #[test]
    fn the_type_icon_is_found_under_the_home_and_painted() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        let meters = theme_into_table(&mut face, "480x320");
        let bar_at = meters.find("[bar]").expect("the bar meter");
        let extended = "[bar]\nconfig.extend = True\nplayinfo.type.pos = 400,20\nplayinfo.type.dimension = 60,40\nplayinfo.type.mode = icon\nplayinfo.type.color = 255,0,0\n";
        let meters = format!("{}{}{}", &meters[..bar_at], extended, &meters[bar_at + 5..]);
        face.put_file("templates/480x320/meters.txt", meters.into_bytes());
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugin/format-icons/cd.svg");
        face.put_file(
            "format-icons/cd.svg",
            std::fs::read(shipped).expect("the shipped icon"),
        );
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","trackType":"cd"}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        let frame = face.frame(40).expect("a frame");
        let painted_red = (20..60).any(|y| {
            (400..460).any(|x| {
                let at = (y * 480 + x) * 4;
                let px = &frame.rgba[at..at + 3];
                px[0] > 200 && px[1] < 60 && px[2] < 60
            })
        });
        assert!(painted_red, "the icon is painted in the area's colour");
        let showing = face.showing.as_ref().expect("the meter on show");
        assert_eq!(showing.metadata.type_icon, "/glass/format-icons/cd.svg");
        assert!(
            showing.pictures.icon().is_some(),
            "the icon is kept between frames"
        );
    }

    /// The bar meter with the keys given, put in the table.
    fn bar_with(face: &mut Face, keys: &str) {
        let meters = theme_into_table(face, "480x320");
        let bar_at = meters.find("[bar]").expect("the bar meter");
        let extended = format!("[bar]\nconfig.extend = True\n{keys}");
        let meters = format!("{}{}{}", &meters[..bar_at], extended, &meters[bar_at + 5..]);
        face.put_file("templates/480x320/meters.txt", meters.into_bytes());
    }

    /// A picture from the test theme, as bytes to hand in for a want.
    fn a_picture() -> Vec<u8> {
        std::fs::read(repo_theme("480x320").join("bar-indicator.png")).expect("a picture")
    }

    /// The first file wanted under `pictures/`, with its URL.
    fn picture_wanted(face: &Face) -> Option<(String, String)> {
        face.wants().into_iter().find_map(|w| match w {
            intake::wants::Want::File { path, url } if path.starts_with("pictures/") => {
                Some((path, url))
            }
            _ => None,
        })
    }

    /// A meter with an album art box: the art the player reports is wanted
    /// from the manager's picture route under a path of its own, drawn once
    /// the page puts it, and a picture the manager has not got is not asked
    /// for again.
    #[test]
    fn the_album_art_is_wanted_from_the_page_and_drawn_once_put() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        bar_with(
            &mut face,
            "albumart.pos = 300,100\nalbumart.dimension = 60,60\n",
        );
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","albumart":"/albumart?path=%2Fmnt%2Fa"}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        face.frame(40).expect("a frame");
        let (path, url) = picture_wanted(&face).expect("the art wanted");
        assert_eq!(
            url,
            "/api/face/picture?at=%2Falbumart%3Fpath%3D%252Fmnt%252Fa"
        );
        assert!(
            face.showing.as_ref().unwrap().pictures.art().is_none(),
            "nothing to draw yet"
        );
        face.put_file(&path, a_picture());
        face.frame(80).expect("a frame");
        let showing = face.showing.as_ref().unwrap();
        assert_eq!(showing.metadata.art_file, format!("/glass/{path}"));
        assert!(
            showing.pictures.art().is_some(),
            "the art is drawn once put"
        );
        assert!(
            picture_wanted(&face).is_none(),
            "a picture put is not wanted again"
        );
        // Another track whose art the manager has not got: wanted once,
        // then marked missing and left alone.
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"Another","albumart":"/albumart?path=%2Fmnt%2Fb"}}"#
        ));
        face.frame(120).expect("a frame");
        let (other, _) = picture_wanted(&face).expect("the other art wanted");
        assert_ne!(other, path);
        face.missing(&other);
        face.frame(160).expect("a frame");
        assert!(
            picture_wanted(&face).is_none(),
            "a missing picture is not asked for again"
        );
        assert!(face.showing.as_ref().unwrap().pictures.art().is_none());
    }

    /// A meter with a fanart slot: the artist's set is wanted from the page,
    /// the answer names the pictures, the picture on show is wanted in turn
    /// and drawn once put.
    #[test]
    fn the_fanart_set_is_asked_of_the_page_and_its_picture_drawn() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        bar_with(&mut face, "fanart.pos = 0,0\nfanart.dimension = 120,80\n");
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","artist":"Someone","uri":"mnt/x/a.flac"}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        face.frame(40).expect("a frame");
        let wanted = face.wants();
        assert!(
            wanted.contains(&intake::wants::Want::Fanart {
                artist: "Someone".into(),
                uri: "mnt/x/a.flac".into()
            }),
            "the set is asked: {wanted:?}"
        );
        face.fanart_answer(
            r#"{"success":true,"images":["glass/fanart/someone/1.jpg"],"interval_ms":0,"transition":"none","transition_ms":600,"order":"sequential"}"#,
        );
        face.frame(80).expect("a frame");
        let (path, url) = picture_wanted(&face).expect("the picture wanted");
        assert_eq!(
            url,
            "/api/face/picture?at=%2Falbumart%3Fsectionimage%3Dglass%2Ffanart%2Fsomeone%2F1.jpg"
        );
        face.put_file(&path, a_picture());
        // The slot decodes off the frame loop on a machine: a few frames.
        let mut drawn = false;
        for i in 0..200 {
            face.frame(120 + i * 20).expect("a frame");
            if face.showing.as_ref().unwrap().pictures.fanart().0.is_some() {
                drawn = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(drawn, "the fanart is drawn once put");
        assert_eq!(
            face.showing.as_ref().unwrap().metadata.fanart_file,
            format!("/glass/{path}")
        );
    }

    /// The slideshow's interval runs between the player's states: with the
    /// set on show and nothing else changing, the set is asked again once
    /// the interval is over, and the answer moves the picture on.
    #[test]
    fn the_slideshow_moves_on_at_its_interval_between_states() {
        let _serial = serial();
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        bar_with(&mut face, "fanart.pos = 0,0\nfanart.dimension = 120,80\n");
        assert!(face.event(
            br#"{"kind":"state","state":{"status":"play","title":"A song","artist":"Else","uri":"mnt/x/b.flac"}}"#
        ));
        face.start(Some("bar")).expect("the bar on show");
        face.frame(40).expect("a frame");
        let set = intake::wants::Want::Fanart {
            artist: "Else".into(),
            uri: "mnt/x/b.flac".into(),
        };
        assert!(face.wants().contains(&set), "the set is asked");
        let answer = r#"{"success":true,"images":["glass/fanart/else/a.jpg","glass/fanart/else/b.jpg"],"interval_ms":300,"transition":"none","transition_ms":600,"order":"sequential"}"#;
        face.fanart_answer(answer);
        face.frame(80).expect("a frame");
        // The picture on show is the one wanted from the page.
        let picture_asked = |face: &Face, name: &str| {
            face.wants()
                .iter()
                .any(|w| matches!(w, intake::wants::Want::File { url, .. } if url.contains(name)))
        };
        assert!(picture_asked(&face, "a.jpg"), "{:?}", face.wants());
        assert!(!picture_asked(&face, "b.jpg"), "{:?}", face.wants());
        assert!(
            !face.wants().contains(&set),
            "nothing asked before the interval"
        );
        // No state arrives; the interval passes, and a second with it.
        std::thread::sleep(std::time::Duration::from_millis(1400));
        face.frame(1500).expect("a frame");
        assert!(
            face.wants().contains(&set),
            "the set is asked again once the interval is over"
        );
        face.fanart_answer(answer);
        face.frame(1540).expect("a frame");
        assert!(picture_asked(&face, "b.jpg"), "{:?}", face.wants());
    }
}
