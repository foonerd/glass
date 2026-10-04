//! Whether this system can draw on a screen of the display's own, tried
//! without drawing: `glass --probe-graphics`.
//!
//! On a screen with no X server the display draws through the kernel's
//! KMS/DRM with GBM and EGL. Where one of them does not work, SDL says one
//! sentence whichever step failed ("Can't load EGL/GL library on window
//! creation"), and libglvnd, the front desk of EGL, says nothing at all
//! when the vendor's library will not load. The probe takes the same steps
//! one by one and says which one fails and why, in words the system's own
//! loader gives: the screen's card is opened, a GBM device made on it, the
//! EGL library loaded, every vendor library the system registers loaded by
//! itself, an EGL display asked for the device and initialised.
//!
//! Nothing is drawn and no mode is set, so it runs while a kiosk holds the
//! screen: the Manager asks before the screen is handed over, and says what
//! it heard instead of leaving a black screen to say it.

use serde::Serialize;
use std::process::ExitCode;

/// One step of the probe: its name, whether it passed, and what was found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step {
    pub step: &'static str,
    pub ok: bool,
    pub detail: String,
}

/// What the probe found. `applies` is false where the kernel drives no
/// screen at all (a player whose picture comes through an X server alone,
/// or another system): there is nothing to probe, and nothing has failed.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Probe {
    pub applies: bool,
    pub ok: bool,
    /// The step that failed, or empty.
    pub failed: String,
    /// One line for a person: what works, or what does not and why.
    pub reason: String,
    /// The kernel driver of the screen's card (`vc4`, `drm-rp1-dsi`,
    /// `panel-mipi-dbi`, `udl`, ...) and the connector that is connected.
    pub driver: String,
    pub connector: String,
    /// Who renders, by the renderer's own name, where it could be asked;
    /// and whether that is the CPU.
    pub renderer: String,
    pub software: bool,
    pub steps: Vec<Step>,
}

impl Probe {
    /// The probe with what was learned of the screen and of who renders:
    /// a probe that passed says it in its reason, since a screen drawn in
    /// software works and is slow, and that is worth a word.
    pub fn told(mut self, driver: &str, connector: &str, renderer: &str) -> Probe {
        self.driver = driver.to_string();
        self.connector = connector.to_string();
        self.renderer = renderer.to_string();
        self.software = software_renderer(renderer);
        if self.applies && self.ok && !renderer.is_empty() {
            self.reason = if self.software {
                format!(
                    "{}; rendered in software, on the processor ({renderer})",
                    self.reason
                )
            } else {
                format!("{}; rendered by {renderer}", self.reason)
            };
        }
        self
    }
}

/// Whether a renderer's name is one of Mesa's software renderers: the
/// screen is drawn on by the processor, with no GPU in it.
pub fn software_renderer(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["llvmpipe", "softpipe", "swrast", "software rasterizer"]
        .iter()
        .any(|word| name.contains(word))
}

/// The connector of `card` that is connected, by its own name (`DSI-2`,
/// `HDMI-A-1`, `SPI-1`), or empty.
pub fn connected_of(connectors: &[(String, String)], card: &str) -> String {
    let prefix = format!("{card}-");
    let mut names: Vec<&str> = connectors
        .iter()
        .filter(|(name, status)| name.starts_with(&prefix) && status.trim() == "connected")
        .map(|(name, _)| &name[prefix.len()..])
        .collect();
    names.sort();
    names.first().map(|n| n.to_string()).unwrap_or_default()
}

/// What to say where the kernel lists no screen: with a framebuffer device
/// there (`framebuffer` is its line of `/proc/fb`, such as `0 fb_ili9341`),
/// the screen is of the kind that is drawn on through an X server only.
pub fn no_card_reason(framebuffer: Option<&str>) -> String {
    match framebuffer.map(str::trim).filter(|line| !line.is_empty()) {
        Some(line) => format!(
            "the screen is a framebuffer device ({line}) the kernel does not drive through KMS: it is drawn on through an X server only"
        ),
        None => "the kernel drives no screen here: nothing to draw on without an X server"
            .to_string(),
    }
}

pub use pane::kms::pick_card;

/// The library a vendor registration names: `ICD.library_path` of a file
/// under `egl_vendor.d`, or `None` when the file says nothing of the kind.
pub fn vendor_library(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let path = value.get("ICD")?.get("library_path")?.as_str()?.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// An EGL error by its name, as the specification calls it.
pub fn egl_error_name(code: i32) -> String {
    let name = match code {
        0x3000 => "EGL_SUCCESS",
        0x3001 => "EGL_NOT_INITIALIZED",
        0x3002 => "EGL_BAD_ACCESS",
        0x3003 => "EGL_BAD_ALLOC",
        0x3004 => "EGL_BAD_ATTRIBUTE",
        0x3005 => "EGL_BAD_CONFIG",
        0x3006 => "EGL_BAD_CONTEXT",
        0x3007 => "EGL_BAD_CURRENT_SURFACE",
        0x3008 => "EGL_BAD_DISPLAY",
        0x3009 => "EGL_BAD_MATCH",
        0x300a => "EGL_BAD_NATIVE_PIXMAP",
        0x300b => "EGL_BAD_NATIVE_WINDOW",
        0x300c => "EGL_BAD_PARAMETER",
        0x300d => "EGL_BAD_SURFACE",
        0x300e => "EGL_CONTEXT_LOST",
        _ => return format!("EGL error 0x{code:04x}"),
    };
    name.to_string()
}

/// The probe's verdict from its steps. The display and its initialising
/// decide: where they pass, the graphics work whatever a vendor library
/// said by itself. Where they fail, the reason is the first step that
/// failed, and a vendor library that would not load is named with it, since
/// that is why EGL has no display to give and nothing else says so.
pub fn conclude(applies: bool, steps: Vec<Step>) -> Probe {
    if !applies {
        return Probe {
            applies: false,
            ok: true,
            failed: String::new(),
            reason: no_card_reason(None),
            steps,
            ..Probe::default()
        };
    }
    let passed = |name: &str| steps.iter().any(|s| s.step == name && s.ok);
    let ok = passed("egl-display") && passed("egl-initialize");
    if ok {
        let found = steps
            .iter()
            .find(|s| s.step == "egl-initialize")
            .map(|s| s.detail.clone())
            .unwrap_or_default();
        return Probe {
            applies: true,
            ok: true,
            failed: String::new(),
            reason: format!("EGL works on the screen's device ({found})"),
            steps,
            ..Probe::default()
        };
    }
    let first = steps.iter().find(|s| !s.ok && s.step != "egl-vendor");
    let vendor = steps.iter().find(|s| !s.ok && s.step == "egl-vendor");
    let failed = first.or(vendor).map(|s| s.step).unwrap_or("egl-display");
    let mut reason = first
        .map(|s| s.detail.clone())
        .unwrap_or_else(|| "EGL gave no display for the screen's device".to_string());
    if let Some(v) = vendor {
        if first.is_none_or(|s| s.step == "egl-display" || s.step == "egl-initialize") {
            reason = format!("{reason}; {}", v.detail);
        }
    }
    Probe {
        applies: true,
        ok: false,
        failed: failed.to_string(),
        reason,
        steps,
        ..Probe::default()
    }
}

/// Run the probe and print it as one line of JSON. The exit is success
/// whatever it found: the answer is in what is printed.
pub fn main() -> ExitCode {
    let probe = run();
    match serde_json::to_string(&probe) {
        Ok(line) => println!("{line}"),
        Err(e) => {
            eprintln!("glass: the probe could not be written: {e}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

#[cfg(not(target_os = "linux"))]
fn run() -> Probe {
    conclude(false, Vec::new())
}

#[cfg(target_os = "linux")]
fn run() -> Probe {
    linux::run()
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{
        conclude, connected_of, egl_error_name, no_card_reason, pick_card, vendor_library, Probe,
        Step,
    };
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::os::fd::AsRawFd;

    /// Where the system registers its EGL vendors, in the order libglvnd
    /// reads them.
    const VENDOR_DIRS: [&str; 2] = ["/etc/glvnd/egl_vendor.d", "/usr/share/glvnd/egl_vendor.d"];
    const EGL_PLATFORM_GBM: u32 = 0x31d7;
    const EGL_VENDOR: c_int = 0x3053;
    const EGL_VERSION: c_int = 0x3054;

    /// A library loaded with every symbol bound now, or the loader's own
    /// words for why not: the file that is not there, the symbol that is
    /// not defined.
    fn load(name: &str) -> Result<*mut c_void, String> {
        let c = CString::new(name).map_err(|_| format!("{name}: not a name"))?;
        // SAFETY: `dlopen` and `dlerror` are called with a valid C string
        // and their answers read at once, on this one thread.
        unsafe {
            libc::dlerror();
            let handle = libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                let why = libc::dlerror();
                let why = if why.is_null() {
                    "the loader gave no reason".to_string()
                } else {
                    CStr::from_ptr(why).to_string_lossy().into_owned()
                };
                Err(why)
            } else {
                Ok(handle)
            }
        }
    }

    /// A function of a loaded library by its name, or null.
    fn symbol(library: *mut c_void, name: &str) -> *mut c_void {
        let Ok(c) = CString::new(name) else {
            return std::ptr::null_mut();
        };
        // SAFETY: `library` is a handle `dlopen` gave and `c` a valid C string.
        unsafe { libc::dlsym(library, c.as_ptr()) }
    }

    /// Every vendor library the system registers, each once, in order.
    fn vendors() -> Vec<String> {
        let mut found = Vec::new();
        for dir in VENDOR_DIRS {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            files.sort();
            for file in files {
                let library = std::fs::read_to_string(&file)
                    .ok()
                    .and_then(|text| vendor_library(&text));
                if let Some(library) = library {
                    if !found.contains(&library) {
                        found.push(library);
                    }
                }
            }
        }
        found
    }

    pub fn run() -> Probe {
        let mut steps = Vec::new();
        let step = |steps: &mut Vec<Step>, step: &'static str, ok: bool, detail: String| {
            steps.push(Step { step, ok, detail });
        };

        // The screen's card, its driver and the connector that is connected.
        let listed = pane::kms::connectors();
        let Some(card) = pick_card(&listed) else {
            let framebuffer = std::fs::read_to_string("/proc/fb")
                .ok()
                .and_then(|text| text.lines().next().map(str::to_string));
            let mut none = conclude(false, steps);
            none.reason = no_card_reason(framebuffer.as_deref());
            return none;
        };
        let driver = std::fs::read_link(format!("/sys/class/drm/{card}/device/driver"))
            .ok()
            .and_then(|link| link.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let connector = connected_of(&listed, &card);
        let told = |probe: Probe, renderer: &str| probe.told(&driver, &connector, renderer);
        let node = format!("/dev/dri/{card}");
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&node)
        {
            Ok(file) => file,
            Err(e) => {
                step(
                    &mut steps,
                    "card",
                    false,
                    format!("{node} could not be opened: {e}"),
                );
                return told(conclude(true, steps), "");
            }
        };
        step(
            &mut steps,
            "card",
            true,
            format!(
                "{node}{}{}",
                if driver.is_empty() {
                    String::new()
                } else {
                    format!(" ({driver})")
                },
                if connector.is_empty() {
                    String::new()
                } else {
                    format!(", {connector} connected")
                }
            ),
        );

        // Whoever opens a card that has no master becomes its master, and a
        // display that starts in that moment would find the card taken and
        // give up. The probe is no display: it lets go of it at once.
        const DRM_IOCTL_DROP_MASTER: u32 = 0x641f;
        // SAFETY: an ioctl with no argument on a descriptor that is open;
        // its failing, where another holds the card, changes nothing.
        unsafe {
            libc::ioctl(file.as_raw_fd(), DRM_IOCTL_DROP_MASTER as _);
        }

        // A GBM device on it.
        let gbm = match load("libgbm.so.1") {
            Ok(library) => library,
            Err(why) => {
                step(
                    &mut steps,
                    "gbm",
                    false,
                    format!("libgbm.so.1 would not load: {why}"),
                );
                return told(conclude(true, steps), "");
            }
        };
        let create = symbol(gbm, "gbm_create_device");
        let destroy = symbol(gbm, "gbm_device_destroy");
        if create.is_null() {
            step(
                &mut steps,
                "gbm",
                false,
                "libgbm.so.1 has no gbm_create_device".to_string(),
            );
            return told(conclude(true, steps), "");
        }
        // SAFETY: the symbol is libgbm's `gbm_create_device(int) -> struct
        // gbm_device *`, called with a descriptor that stays open below.
        let device = unsafe {
            let create: unsafe extern "C" fn(c_int) -> *mut c_void = std::mem::transmute(create);
            create(file.as_raw_fd())
        };
        if device.is_null() {
            step(
                &mut steps,
                "gbm",
                false,
                format!("no GBM device on {node}: the graphics driver for it did not load"),
            );
            return told(conclude(true, steps), "");
        }
        step(&mut steps, "gbm", true, format!("a GBM device on {node}"));

        // The vendors the system registers, each loaded by itself: what
        // libglvnd would do, and say nothing of when it fails.
        let registered = vendors();
        if registered.is_empty() {
            step(
                &mut steps,
                "egl-vendor",
                true,
                "no vendor registered under egl_vendor.d".to_string(),
            );
        }
        for library in &registered {
            match load(library) {
                Ok(_) => step(&mut steps, "egl-vendor", true, format!("{library} loads")),
                Err(why) => step(
                    &mut steps,
                    "egl-vendor",
                    false,
                    format!("the EGL vendor library {library} would not load: {why}"),
                ),
            }
        }

        // EGL itself, a display for the device, and its initialising.
        let egl = match load("libEGL.so.1") {
            Ok(library) => library,
            Err(why) => {
                step(
                    &mut steps,
                    "egl-library",
                    false,
                    format!("libEGL.so.1 would not load: {why}"),
                );
                return told(conclude(true, steps), "");
            }
        };
        let get_display = symbol(egl, "eglGetDisplay");
        let get_platform_display = symbol(egl, "eglGetPlatformDisplay");
        let initialize = symbol(egl, "eglInitialize");
        let get_error = symbol(egl, "eglGetError");
        let terminate = symbol(egl, "eglTerminate");
        let query_string = symbol(egl, "eglQueryString");
        if get_display.is_null() || initialize.is_null() || get_error.is_null() {
            step(
                &mut steps,
                "egl-library",
                false,
                "libEGL.so.1 lacks eglGetDisplay, eglInitialize or eglGetError".to_string(),
            );
            return told(conclude(true, steps), "");
        }
        step(
            &mut steps,
            "egl-library",
            true,
            "libEGL.so.1 loads".to_string(),
        );

        let mut renderer = String::new();
        // SAFETY: each pointer is the EGL function of that name from the
        // library just loaded, called as the EGL specification declares it,
        // with the GBM device made above as the native display.
        unsafe {
            let get_display: unsafe extern "C" fn(*mut c_void) -> *mut c_void =
                std::mem::transmute(get_display);
            let initialize: unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> u32 =
                std::mem::transmute(initialize);
            let get_error: unsafe extern "C" fn() -> c_int = std::mem::transmute(get_error);
            let mut display = std::ptr::null_mut();
            if !get_platform_display.is_null() {
                let get_platform_display: unsafe extern "C" fn(
                    u32,
                    *mut c_void,
                    *const isize,
                ) -> *mut c_void = std::mem::transmute(get_platform_display);
                display = get_platform_display(EGL_PLATFORM_GBM, device, std::ptr::null());
            }
            if display.is_null() {
                display = get_display(device);
            }
            if display.is_null() {
                step(
                    &mut steps,
                    "egl-display",
                    false,
                    format!(
                        "EGL gave no display for {node} ({})",
                        egl_error_name(get_error())
                    ),
                );
            } else {
                step(
                    &mut steps,
                    "egl-display",
                    true,
                    format!("an EGL display for {node}"),
                );
                let (mut major, mut minor) = (0, 0);
                if initialize(display, &mut major, &mut minor) == 1 {
                    let mut found = format!("EGL {major}.{minor}");
                    if !query_string.is_null() {
                        let query_string: unsafe extern "C" fn(
                            *mut c_void,
                            c_int,
                        )
                            -> *const c_char = std::mem::transmute(query_string);
                        for name in [EGL_VENDOR, EGL_VERSION] {
                            let text = query_string(display, name);
                            if !text.is_null() {
                                found =
                                    format!("{found}, {}", CStr::from_ptr(text).to_string_lossy());
                            }
                        }
                    }
                    step(&mut steps, "egl-initialize", true, found);
                    renderer = renderer_name(egl, display).unwrap_or_default();
                    if !terminate.is_null() {
                        let terminate: unsafe extern "C" fn(*mut c_void) -> u32 =
                            std::mem::transmute(terminate);
                        terminate(display);
                    }
                } else {
                    step(
                        &mut steps,
                        "egl-initialize",
                        false,
                        format!(
                            "the EGL display for {node} would not initialise ({})",
                            egl_error_name(get_error())
                        ),
                    );
                }
            }
            if !destroy.is_null() {
                let destroy: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(destroy);
                destroy(device);
            }
        }
        drop(file);
        told(conclude(true, steps), &renderer)
    }

    /// Who renders on `display`, by the renderer's own name: a context is
    /// made with no surface, OpenGL ES 2 or else OpenGL, and asked. `None`
    /// where any of it cannot be had; the probe's verdict does not hang on
    /// it.
    ///
    /// # Safety
    /// `egl` is the handle of a loaded libEGL and `display` an initialised
    /// display of it.
    unsafe fn renderer_name(egl: *mut c_void, display: *mut c_void) -> Option<String> {
        const EGL_NONE: c_int = 0x3038;
        const EGL_SURFACE_TYPE: c_int = 0x3033;
        const EGL_WINDOW_BIT: c_int = 0x0004;
        const EGL_RENDERABLE_TYPE: c_int = 0x3040;
        const EGL_CONTEXT_CLIENT_VERSION: c_int = 0x3098;
        const GL_RENDERER: u32 = 0x1f01;
        // (the API to bind, its bit among the renderable types, a client version or none)
        const APIS: [(u32, c_int, c_int); 2] = [(0x30a0, 0x0004, 2), (0x30a2, 0x0008, 0)];
        let function = |name: &str| {
            let pointer = symbol(egl, name);
            (!pointer.is_null()).then_some(pointer)
        };
        let bind_api: unsafe extern "C" fn(u32) -> u32 =
            std::mem::transmute(function("eglBindAPI")?);
        let choose_config: unsafe extern "C" fn(
            *mut c_void,
            *const c_int,
            *mut *mut c_void,
            c_int,
            *mut c_int,
        ) -> u32 = std::mem::transmute(function("eglChooseConfig")?);
        let create_context: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *const c_int,
        ) -> *mut c_void = std::mem::transmute(function("eglCreateContext")?);
        let make_current: unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            *mut c_void,
            *mut c_void,
        ) -> u32 = std::mem::transmute(function("eglMakeCurrent")?);
        let destroy_context: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32 =
            std::mem::transmute(function("eglDestroyContext")?);
        let get_proc_address: unsafe extern "C" fn(*const c_char) -> *mut c_void =
            std::mem::transmute(function("eglGetProcAddress")?);
        for (api, bit, version) in APIS {
            if bind_api(api) != 1 {
                continue;
            }
            let wanted = [
                EGL_SURFACE_TYPE,
                EGL_WINDOW_BIT,
                EGL_RENDERABLE_TYPE,
                bit,
                EGL_NONE,
            ];
            let mut config = std::ptr::null_mut();
            let mut count = 0;
            if choose_config(display, wanted.as_ptr(), &mut config, 1, &mut count) != 1 || count < 1
            {
                continue;
            }
            let attributes = [EGL_CONTEXT_CLIENT_VERSION, version, EGL_NONE];
            let attributes = if version > 0 {
                attributes.as_ptr()
            } else {
                attributes[2..].as_ptr()
            };
            let context = create_context(display, config, std::ptr::null_mut(), attributes);
            if context.is_null() {
                continue;
            }
            let none = std::ptr::null_mut();
            let mut name = None;
            if make_current(display, none, none, context) == 1 {
                let get_string = get_proc_address(c"glGetString".as_ptr());
                if !get_string.is_null() {
                    let get_string: unsafe extern "C" fn(u32) -> *const c_char =
                        std::mem::transmute(get_string);
                    let text = get_string(GL_RENDERER);
                    if !text.is_null() {
                        name = Some(CStr::from_ptr(text).to_string_lossy().into_owned());
                    }
                }
                make_current(display, none, none, none);
            }
            destroy_context(display, context);
            if name.is_some() {
                return name;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector(name: &str, status: &str) -> (String, String) {
        (name.to_string(), status.to_string())
    }

    #[test]
    fn a_vendor_registration_names_its_library() {
        let mesa = r#"{"file_format_version":"1.0.0","ICD":{"library_path":"libEGL_mesa.so.0"}}"#;
        assert_eq!(vendor_library(mesa).as_deref(), Some("libEGL_mesa.so.0"));
        assert_eq!(vendor_library(r#"{"ICD":{}}"#), None);
        assert_eq!(vendor_library(r#"{"ICD":{"library_path":" "}}"#), None);
        assert_eq!(vendor_library("not json"), None);
    }

    #[test]
    fn egl_errors_are_named() {
        assert_eq!(egl_error_name(0x3001), "EGL_NOT_INITIALIZED");
        assert_eq!(egl_error_name(0x3008), "EGL_BAD_DISPLAY");
        assert_eq!(egl_error_name(0x1234), "EGL error 0x1234");
    }

    #[test]
    fn the_screen_is_named_and_who_renders_is_said() {
        let pi = [
            connector("card2-HDMI-A-1", "disconnected\n"),
            connector("card1-DSI-2", "connected\n"),
        ];
        assert_eq!(connected_of(&pi, "card1"), "DSI-2");
        assert_eq!(connected_of(&pi, "card2"), "");
        assert!(software_renderer("llvmpipe (LLVM 15.0.6, 128 bits)"));
        assert!(software_renderer("softpipe"));
        assert!(!software_renderer("V3D 7.1.10.2"));
        assert!(!software_renderer(""));
        let works = || {
            conclude(
                true,
                vec![
                    step("egl-display", true, "an EGL display"),
                    step("egl-initialize", true, "EGL 1.5"),
                ],
            )
        };
        let gpu = works().told("drm-rp1-dsi", "DSI-2", "V3D 7.1.10.2");
        assert_eq!(
            (gpu.driver.as_str(), gpu.connector.as_str(), gpu.software),
            ("drm-rp1-dsi", "DSI-2", false)
        );
        assert_eq!(
            gpu.reason,
            "EGL works on the screen's device (EGL 1.5); rendered by V3D 7.1.10.2"
        );
        // A screen Mesa has no renderer for is drawn on all the same, by the processor.
        let soft = works().told("gud", "USB-1", "llvmpipe (LLVM 15.0.6, 128 bits)");
        assert!(soft.ok && soft.software);
        assert_eq!(
            soft.reason,
            "EGL works on the screen's device (EGL 1.5); rendered in software, on the processor (llvmpipe (LLVM 15.0.6, 128 bits))"
        );
        // The renderer could not be asked: nothing is claimed of it.
        let unknown = works().told("vc4", "HDMI-A-1", "");
        assert_eq!(unknown.reason, "EGL works on the screen's device (EGL 1.5)");
        assert!(!unknown.software);
        // A probe that failed keeps its reason.
        let broken =
            conclude(true, vec![step("gbm", false, "no GBM device")]).told("vc4", "HDMI-A-1", "");
        assert_eq!(broken.reason, "no GBM device");
    }

    #[test]
    fn a_framebuffer_only_screen_is_said_to_be_one() {
        assert_eq!(
            no_card_reason(Some("0 fb_ili9341\n")),
            "the screen is a framebuffer device (0 fb_ili9341) the kernel does not drive through KMS: it is drawn on through an X server only"
        );
        assert_eq!(
            no_card_reason(None),
            "the kernel drives no screen here: nothing to draw on without an X server"
        );
        assert_eq!(no_card_reason(Some("  ")), no_card_reason(None));
    }

    fn step(step: &'static str, ok: bool, detail: &str) -> Step {
        Step {
            step,
            ok,
            detail: detail.to_string(),
        }
    }

    #[test]
    fn the_verdict_is_the_displays_and_names_what_would_not_load() {
        // A working player.
        let works = conclude(
            true,
            vec![
                step("card", true, "/dev/dri/card1"),
                step("gbm", true, "a GBM device on /dev/dri/card1"),
                step("egl-vendor", true, "libEGL_mesa.so.0 loads"),
                step("egl-library", true, "libEGL.so.1 loads"),
                step("egl-display", true, "an EGL display for /dev/dri/card1"),
                step("egl-initialize", true, "EGL 1.5, Mesa Project, 1.5"),
            ],
        );
        assert!(works.ok && works.applies && works.failed.is_empty());
        assert_eq!(
            works.reason,
            "EGL works on the screen's device (EGL 1.5, Mesa Project, 1.5)"
        );
        // The case this was written for: GBM works, the vendor library is
        // there and will not load, and EGL has no display to give.
        let broken = conclude(
            true,
            vec![
                step("card", true, "/dev/dri/card1"),
                step("gbm", true, "a GBM device on /dev/dri/card1"),
                step(
                    "egl-vendor",
                    false,
                    "the EGL vendor library libEGL_mesa.so.0 would not load: libxshmfence.so.1: cannot open shared object file: No such file or directory",
                ),
                step("egl-library", true, "libEGL.so.1 loads"),
                step(
                    "egl-display",
                    false,
                    "EGL gave no display for /dev/dri/card1 (EGL_SUCCESS)",
                ),
            ],
        );
        assert!(!broken.ok && broken.applies);
        assert_eq!(broken.failed, "egl-display");
        assert_eq!(
            broken.reason,
            "EGL gave no display for /dev/dri/card1 (EGL_SUCCESS); the EGL vendor library libEGL_mesa.so.0 would not load: libxshmfence.so.1: cannot open shared object file: No such file or directory"
        );
        // A vendor library that would not load by itself does not fail a
        // player whose display initialises all the same.
        let odd = conclude(
            true,
            vec![
                step(
                    "egl-vendor",
                    false,
                    "the EGL vendor library libEGL_other.so.0 would not load: x",
                ),
                step("egl-display", true, "an EGL display"),
                step("egl-initialize", true, "EGL 1.5"),
            ],
        );
        assert!(odd.ok);
        // An earlier step that fails is the reason by itself.
        let no_card = conclude(
            true,
            vec![step(
                "card",
                false,
                "/dev/dri/card1 could not be opened: Permission denied",
            )],
        );
        assert_eq!(no_card.failed, "card");
        assert_eq!(
            no_card.reason,
            "/dev/dri/card1 could not be opened: Permission denied"
        );
        // No screen the kernel drives: nothing to probe, nothing failed.
        let none = conclude(false, Vec::new());
        assert!(none.ok && !none.applies);
    }
}
