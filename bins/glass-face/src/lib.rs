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
//!
//! The raw exports at the end wrap [`Face`] for a host without bindings.

use std::cell::RefCell;
use std::rc::Rc;

use expose::{raster_over, MeterAssets, Motion, Stack};
use intake::bring::{
    asset_plan, config_texts, theme_plan, Bring, Choice, RemoteConfig, ThemeFiles,
};
use intake::hops::WireHops;
use intake::{decode_event, Hops, Source, Taken, TapSource};
use lead::{vfs, SkinDesc};

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
}

/// The pipeline for a page.
#[derive(Default)]
pub struct Face {
    hops: SharedHops,
    theme: String,
    showing: Option<Showing>,
    /// Events that arrived before the meter was on show, kept for it.
    early: Vec<intake::Event>,
}

impl Face {
    pub fn new() -> Self {
        lead::set_home(HOME);
        Self::default()
    }

    /// A file under its path in the home, as the plans name it.
    pub fn put_file(&self, relative: &str, bytes: Vec<u8>) {
        vfs::put(
            &format!("{HOME}/{}", relative.trim_start_matches('/')),
            bytes,
        );
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
        let skin = intake::installed_skin_named(meter);
        if skin.theme_dir.is_empty() {
            return Err("the configuration names no theme".to_string());
        }
        if !lead::is_file(&std::path::Path::new(&skin.theme_dir).join("meters.txt")) {
            return Err(format!("no meters.txt under {}", skin.theme_dir));
        }
        let assets = MeterAssets::load(&skin);
        let bins = skin
            .spectrum
            .as_ref()
            .map_or(lead::DEFAULT_SPECTRUM_BINS, |s| s.bins.max(1));
        let mut source = TapSource::new(bins, skin.meter_max)
            .without_player()
            .with_hops(Box::new(self.hops.clone()))
            .with_skin(&skin);
        for event in self.early.drain(..) {
            source.push_event(event);
        }
        let rate = intake::installed_frame_rate();
        self.showing = Some(Showing {
            skin,
            assets,
            source,
            motion: Motion::new(1, Some(rate)),
            rate,
        });
        Ok(())
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
        let showing = self.showing.as_mut()?;
        let input = showing.source.poll();
        let scene = plot::step(&showing.skin, &input);
        let assets = &showing.assets;
        let stack = Stack {
            screen: None,
            face: None,
            front: assets.front.as_ref(),
            needle: assets.indicator.as_ref(),
            needle_right: assets.indicator_right.as_ref(),
            face_at: showing.skin.face_at,
            fonts: Some(&assets.fonts),
            art: None,
            icon: None,
            spectrum: assets.spectrum.as_ref(),
            folder_pictures: &[],
            fanart: (None, None),
            vinyl: None,
            tonearm: assets.tonearm.as_ref(),
            reels: (assets.reels.0.as_ref(), assets.reels.1.as_ref()),
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
        FACE.with(|face| face.borrow().put_file(&path, bytes));
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
        let mut face = Face::new();
        face.configure(&config_json()).expect("a plan");
        let dir = repo_theme("480x320");
        for entry in std::fs::read_dir(&dir).expect("the test theme") {
            let path = entry.expect("an entry").path();
            if path.is_file() {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                face.put_file(
                    &format!("templates/480x320/{name}"),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
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
}
