//! Put a finished frame on a surface, or send a scene to another glass.
//! This station does not decide angles or bar heights.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use expose::{Frame, Rect};
use plot::Scene;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{FullscreenType, Window, WindowContext, WindowPos};
use sdl2::EventPump;

/// What happened while a frame went up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// The frame is up and the window stays.
    Kept,
    /// The window was closed from outside.
    Closed,
    /// A finger or a mouse button was lifted on the window.
    Touched,
    /// Escape: the window should leave full screen, so the desktop behind
    /// it can be used, and keep showing.
    LeaveFullscreen,
    /// F: the window should take the screen again.
    EnterFullscreen,
}

/// A pointer event, in the frame's own pixels: a finger or a mouse
/// button going down, moving while down, or lifting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pub kind: PointerKind,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
}

/// How the window sits on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowMode {
    /// The whole screen, no frame: the player's own display.
    #[default]
    Fullscreen,
    /// A window with a title bar, movable.
    Windowed,
    /// A window without a frame, at a fixed place.
    Frameless,
}

/// What a window is opened with.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowOptions {
    pub mode: WindowMode,
    /// Where the window goes in windowed and frameless modes; centred otherwise.
    pub position: Option<(i32, i32)>,
    /// Scale the frame to the window, keeping its shape, instead of
    /// showing it pixel for pixel: a theme larger or smaller than the screen fills it.
    pub fit: bool,
    /// Not fitted, the frame's size as a share of its own, [`MIN_SCALE_PERCENT`]
    /// to 100 percent: a theme shown smaller than it is, never larger, so
    /// nothing is drawn jagged. A window that is not full screen is made
    /// at that size.
    pub scale: u32,
    /// Not fitted, where the frame sits in a window it does not fill.
    pub anchor: Anchor,
    /// Not fitted, how far the frame keeps off the edges it sits against.
    pub margins: Margins,
    /// Show the pointer.
    pub pointer: bool,
    /// Escape or Q closes the window.
    pub keys: bool,
    pub title: String,
    /// The screen the window opens on, by SDL's count from 0.
    pub display: u32,
    /// What SDL draws with, by its name (`x11`, `wayland`, `kmsdrm`);
    /// none lets SDL choose.
    pub driver: Option<String>,
    /// The picture turned on the screen, clockwise, by 0, 90, 180 or 270
    /// degrees; touch is turned the same way.
    pub rotation: u32,
    /// A finger's share of the panel through this before it becomes a
    /// pixel: `(ax + by + c, dx + ey + f)`, for a panel whose touch frame
    /// is not the picture's. The identity changes nothing.
    pub touch_matrix: [f32; 6],
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            mode: WindowMode::Fullscreen,
            position: None,
            fit: false,
            scale: 100,
            anchor: Anchor::Center,
            margins: Margins::default(),
            pointer: false,
            keys: false,
            title: "Glass".to_string(),
            display: 0,
            driver: None,
            rotation: 0,
            touch_matrix: IDENTITY,
        }
    }
}

/// The matrix that changes nothing.
pub const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// The smallest share of its own size a frame is shown at.
pub const MIN_SCALE_PERCENT: u32 = 10;

/// Where a frame that does not fill its window sits: the centre, an edge
/// or a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// A frame's end of one axis: at the start, in the middle, at the end.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Near,
    Middle,
    Far,
}

impl Anchor {
    fn across(self) -> Side {
        match self {
            Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => Side::Near,
            Anchor::Top | Anchor::Center | Anchor::Bottom => Side::Middle,
            Anchor::TopRight | Anchor::Right | Anchor::BottomRight => Side::Far,
        }
    }

    fn down(self) -> Side {
        match self {
            Anchor::TopLeft | Anchor::Top | Anchor::TopRight => Side::Near,
            Anchor::Left | Anchor::Center | Anchor::Right => Side::Middle,
            Anchor::BottomLeft | Anchor::Bottom | Anchor::BottomRight => Side::Far,
        }
    }
}

/// A frame's size at a share of its own, the share kept within
/// [`MIN_SCALE_PERCENT`] and 100.
pub fn scaled_size(frame: (u32, u32), percent: u32) -> (u32, u32) {
    let scale = percent.clamp(MIN_SCALE_PERCENT, 100) as f32 / 100.0;
    (
        (frame.0.max(1) as f32 * scale).round().max(1.0) as u32,
        (frame.1.max(1) as f32 * scale).round().max(1.0) as u32,
    )
}

/// Where a frame shown at a share of its own size goes in a window: that
/// share both ways, never larger than itself, with its top left at
/// `placement`, else where the anchor puts it; and the scale it took. At
/// 100 and centred this is the frame pixel for pixel in the middle.
pub fn placed_rect(
    frame: (u32, u32),
    window: (u32, u32),
    placement: Option<(i32, i32)>,
    percent: u32,
    anchor: Anchor,
) -> (i32, i32, u32, u32, f32) {
    placed_rect_within(
        frame,
        window,
        placement,
        percent,
        anchor,
        Margins::default(),
    )
}

/// The largest margin, as a share of the window's shorter side.
pub const MAX_MARGIN_PERCENT: u32 = 25;

/// How far a frame keeps off the edges its anchor puts it against, each
/// as a share of the window's shorter side, 0 to [`MAX_MARGIN_PERCENT`]
/// percent. A centred frame keeps off nothing: there is no edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Margins {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Margins {
    /// The same share off every edge.
    pub fn all(percent: u32) -> Self {
        Self {
            top: percent,
            right: percent,
            bottom: percent,
            left: percent,
        }
    }

    /// Each share kept within its bounds.
    pub fn clamped(self) -> Self {
        Self {
            top: self.top.min(MAX_MARGIN_PERCENT),
            right: self.right.min(MAX_MARGIN_PERCENT),
            bottom: self.bottom.min(MAX_MARGIN_PERCENT),
            left: self.left.min(MAX_MARGIN_PERCENT),
        }
    }
}

/// [`placed_rect`] with margins: a frame against an edge keeps that edge's
/// margin, and never goes past the opposite edge for it; a frame larger
/// than the window sits as it does without margins.
pub fn placed_rect_within(
    frame: (u32, u32),
    window: (u32, u32),
    placement: Option<(i32, i32)>,
    percent: u32,
    anchor: Anchor,
    margins: Margins,
) -> (i32, i32, u32, u32, f32) {
    let scale = percent.clamp(MIN_SCALE_PERCENT, 100) as f32 / 100.0;
    let (w, h) = scaled_size(frame, percent);
    let margins = margins.clamped();
    let shorter = window.0.min(window.1) as f32;
    let px = |share: u32| -> i32 { (shorter * share as f32 / 100.0).round() as i32 };
    let along = |side: Side, room: u32, size: u32, near: u32, far: u32| -> i32 {
        let spare = room as i32 - size as i32;
        match side {
            Side::Near => px(near).min(spare.max(0)),
            Side::Middle => spare / 2,
            Side::Far => {
                if spare < 0 {
                    spare
                } else {
                    (spare - px(far)).max(0)
                }
            }
        }
    };
    let (x, y) = placement.unwrap_or((
        along(anchor.across(), window.0, w, margins.left, margins.right),
        along(anchor.down(), window.1, h, margins.top, margins.bottom),
    ));
    (x, y, w, h, scale)
}

/// One window. The binary keeps it for the life of the player, with one
/// streaming texture the frames are uploaded into.
pub struct Surface {
    canvas: Canvas<Window>,
    creator: TextureCreator<WindowContext>,
    texture: Option<Texture>,
    /// The picture under a frame that leaves part of the window bare, the
    /// window's size; none shows the black.
    backdrop: Option<Texture>,
    /// The backlight the kernel lists for the panel, where there is one.
    backlight: Option<std::path::PathBuf>,
    /// The panel's power as last switched, and by what means.
    power: Power,
    pump: EventPump,
    /// Where the frame's top left goes; `None` centres it in the window.
    placement: Option<(i32, i32)>,
    fit: bool,
    /// Not fitted, the frame's share of its own size, where it sits, and
    /// how far it keeps off the edges.
    scale: u32,
    anchor: Anchor,
    margins: Margins,
    keys: bool,
    /// What draws the frames: the video driver the window opened on, the
    /// renderer's name, and whether it draws in software.
    driver: String,
    renderer: String,
    software: bool,
    /// The frame size the window was made or fitted for.
    frame_size: (u32, u32),
    /// Pointer events since the last take, in frame pixels.
    pointer: Vec<Pointer>,
    /// Where the frame's top left sat in the window at the last show, and
    /// the scale it was shown at (1 when not fitted), for mapping window
    /// pixels to frame pixels.
    last_offset: (i32, i32),
    last_scale: f32,
    /// The window's own size at the last show, in its own pixels.
    last_window: (u32, u32),
    /// The picture's turn on the screen, clockwise degrees.
    rotation: u32,
    /// Shows so far, for the grab.
    shows: u32,
    /// A file to write the window's pixels to, once, at that show.
    grab: Option<(std::path::PathBuf, u32)>,
    /// A finger has arrived as a finger: from then on the mouse events SDL
    /// makes up from touches are dropped, so a touch counts once.
    fingers_seen: bool,
    /// The panel's touch frame set right, applied to every finger's share.
    touch_matrix: [f32; 6],
    /// Where fingers lifted, as raw shares of the panel before the matrix,
    /// since the last take: what a calibration reads.
    raw_lifts: Vec<(f32, f32)>,
    /// Whether lifts are being kept: only while a calibration asks, so a
    /// touch from before its targets appeared is never taken for one.
    recording_lifts: bool,
}

/// The screen's card, for a display that draws on the screen itself.
///
/// SDL finds the card by going through `/dev/dri` in the order the system
/// lists it, and its search does not stop at the card it can use: a card
/// listed after it that has connectors and nothing connected, the HDMI
/// card of a Raspberry Pi whose panel is on DSI, makes it forget what it
/// found, and it ends with "kmsdrm not available". The order is the order
/// the kernel made the cards in, which changes from one boot to the next,
/// so the same player draws on its screen after one start and not after
/// another. SDL takes the card's number from `SDL_KMSDRM_DEVICE_INDEX`
/// where that is set; the display sets it, from the connectors the kernel
/// lists, and the search is not run.
pub mod kms {
    /// The connectors the kernel lists, as `(cardN-NAME, status)`.
    #[cfg(target_os = "linux")]
    pub fn connectors() -> Vec<(String, String)> {
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let status = std::fs::read_to_string(e.path().join("status")).ok()?;
                Some((name, status))
            })
            .collect()
    }

    /// The connectors that belong to a card, by name: `card1-DSI-2`.
    fn of_cards(connectors: &[(String, String)]) -> Vec<&(String, String)> {
        let mut sorted: Vec<&(String, String)> = connectors
            .iter()
            .filter(|(name, _)| name.starts_with("card") && name.contains('-'))
            .collect();
        sorted.sort();
        sorted
    }

    fn card_of(name: &str) -> Option<String> {
        name.split('-').next().map(str::to_string)
    }

    /// The card with a screen connected: of the first connector, by name,
    /// that is connected. `None` where nothing is connected.
    pub fn connected_card(connectors: &[(String, String)]) -> Option<String> {
        of_cards(connectors)
            .iter()
            .find(|(_, status)| status.trim() == "connected")
            .and_then(|(name, _)| card_of(name))
    }

    /// The card to look at: the one with a screen connected, else the
    /// first that has a connector at all. `None` where the kernel lists no
    /// connector: it drives no screen.
    pub fn pick_card(connectors: &[(String, String)]) -> Option<String> {
        connected_card(connectors).or_else(|| {
            of_cards(connectors)
                .first()
                .and_then(|(name, _)| card_of(name))
        })
    }

    /// A card's number, as SDL counts the cards: `card1` is 1.
    pub fn card_index(card: &str) -> Option<u32> {
        card.strip_prefix("card")?.parse().ok()
    }

    /// The number of the card with the connected screen, for SDL.
    #[cfg(target_os = "linux")]
    pub fn screen_card_index() -> Option<u32> {
        connected_card(&connectors()).and_then(|card| card_index(&card))
    }
}

/// Whether an SDL video driver shows nothing on any screen: the ones made
/// for drawing with no screen at all.
fn shows_nothing(driver: &str) -> bool {
    matches!(driver, "offscreen" | "dummy")
}

/// Whether a driver was asked for by name, as `SDL_VIDEODRIVER` holds it:
/// one name, or several parted by commas, tried in order.
fn asked_for(asked: &str, driver: &str) -> bool {
    asked
        .split(',')
        .any(|name| name.trim().eq_ignore_ascii_case(driver))
}

impl Surface {
    /// Open the player's window: the whole screen, no frame, no pointer.
    pub fn open(width: u32, height: u32) -> Result<Self, String> {
        Self::open_with(width, height, &WindowOptions::default())
    }

    /// Change how the open window sits: full screen takes the screen it is
    /// on; windowed shows the frame at its size, centred, with a frame;
    /// frameless the same without one.
    pub fn set_mode(&mut self, mode: WindowMode) -> Result<(), String> {
        let (width, height) = if self.fit {
            self.frame_size
        } else {
            scaled_size(self.frame_size, self.scale)
        };
        let window = self.canvas.window_mut();
        match mode {
            WindowMode::Fullscreen => {
                window.set_bordered(false);
                window
                    .set_fullscreen(FullscreenType::Desktop)
                    .map_err(|err| err.to_string())?;
            }
            WindowMode::Windowed | WindowMode::Frameless => {
                window
                    .set_fullscreen(FullscreenType::Off)
                    .map_err(|err| err.to_string())?;
                window.set_bordered(mode == WindowMode::Windowed);
                window
                    .set_size(width.max(1), height.max(1))
                    .map_err(|err| err.to_string())?;
                window.set_position(WindowPos::Centered, WindowPos::Centered);
            }
        }
        Ok(())
    }

    /// Keep a pointer event in frame pixels: the frame's offset comes off,
    /// and the scale a fitted or a smaller-shown frame took.
    fn push_pointer(&mut self, kind: PointerKind, x: i32, y: i32) {
        let (x, y) = unrotate_point((x, y), self.rotation, self.last_window);
        let (x, y) = (x - self.last_offset.0, y - self.last_offset.1);
        let (x, y) = if self.last_scale > 0.0 && self.last_scale != 1.0 {
            (
                (x as f32 / self.last_scale).round() as i32,
                (y as f32 / self.last_scale).round() as i32,
            )
        } else {
            (x, y)
        };
        if self.pointer.len() < 256 {
            self.pointer.push(Pointer { kind, x, y });
        }
    }

    /// The pointer events since the last take.
    pub fn take_pointer(&mut self) -> Vec<Pointer> {
        std::mem::take(&mut self.pointer)
    }

    /// The screens this machine has, by index: name, width and height.
    pub fn monitors(&self) -> Vec<(u32, String, u32, u32)> {
        let video = self.canvas.window().subsystem();
        let count = video.num_video_displays().unwrap_or(0).max(0) as u32;
        (0..count)
            .map(|i| {
                let name = video.display_name(i as i32).unwrap_or_default();
                let (w, h) = video
                    .display_bounds(i as i32)
                    .map(|b| (b.width(), b.height()))
                    .unwrap_or((0, 0));
                (i, name, w, h)
            })
            .collect()
    }

    /// Open a window as the options say. Fails when SDL cannot start.
    pub fn open_with(width: u32, height: u32, options: &WindowOptions) -> Result<Self, String> {
        // The driver is chosen before SDL starts; a name from the settings
        // beats what the launcher's environment says.
        if let Some(name) = options.driver.as_deref() {
            std::env::set_var("SDL_VIDEODRIVER", name);
        }
        let rotation = match options.rotation {
            90 | 180 | 270 => options.rotation,
            _ => 0,
        };
        // SDL is told which card has the screen: its own search for it gives
        // up or not by the order the system lists the cards in (see `kms`).
        // A number set from outside stands.
        #[cfg(target_os = "linux")]
        if std::env::var_os("SDL_KMSDRM_DEVICE_INDEX").is_none() {
            if let Some(index) = kms::screen_card_index() {
                std::env::set_var("SDL_KMSDRM_DEVICE_INDEX", index.to_string());
            }
        }
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        // With no X server, no Wayland and no screen of its own to open,
        // SDL falls back to a driver that draws to nowhere: the display
        // would run and show nothing. That is a screen it could not open.
        let chosen = video.current_video_driver();
        let asked = std::env::var("SDL_VIDEODRIVER").unwrap_or_default();
        if shows_nothing(chosen) && !asked_for(&asked, chosen) {
            return Err(format!(
                "no screen to draw on: no X server, no Wayland and no KMS/DRM device (SDL fell back to {chosen})"
            ));
        }
        let (width, height) = (width.max(1), height.max(1));
        // A window that is not full screen is the frame's size, or the
        // size the frame is shown at when it is shown smaller.
        let (made_w, made_h) = if options.fit {
            (width, height)
        } else {
            scaled_size((width, height), options.scale)
        };
        let mut builder = video.window(&options.title, made_w, made_h);
        // Centred on the chosen screen; a full screen window takes that screen.
        let centred = (sdl2::sys::SDL_WINDOWPOS_CENTERED_MASK | options.display.min(15)) as i32;
        match options.mode {
            WindowMode::Fullscreen => {
                builder
                    .position(centred, centred)
                    .fullscreen_desktop()
                    .borderless();
            }
            WindowMode::Windowed => match options.position {
                Some((x, y)) => {
                    builder.position(x, y);
                }
                None => {
                    builder.position(centred, centred);
                }
            },
            WindowMode::Frameless => {
                match options.position {
                    Some((x, y)) => {
                        builder.position(x, y);
                    }
                    None => {
                        builder.position(centred, centred);
                    }
                }
                builder.borderless();
            }
        }
        let window = builder.build().map_err(|err| err.to_string())?;
        let canvas = window
            .into_canvas()
            .build()
            .map_err(|err| err.to_string())?;
        // Say what draws the frames, so a report from a player tells whether
        // the upload goes through hardware.
        let info = canvas.info();
        println!(
            "glass: renderer {} on {}{}",
            info.name,
            video.current_video_driver(),
            if rotation == 0 {
                String::new()
            } else {
                format!(", turned {rotation}")
            }
        );
        let driver = video.current_video_driver().to_string();
        let renderer = info.name.to_string();
        let software =
            info.flags & (sdl2::sys::SDL_RendererFlags::SDL_RENDERER_SOFTWARE as u32) != 0;
        let creator = canvas.texture_creator();
        let pump = sdl.event_pump()?;
        sdl.mouse().show_cursor(options.pointer);
        Ok(Self {
            canvas,
            creator,
            texture: None,
            backdrop: None,
            backlight: backlight::find(),
            power: Power { on: true, by: None },
            pump,
            placement: None,
            fit: options.fit,
            scale: options.scale.clamp(MIN_SCALE_PERCENT, 100),
            anchor: options.anchor,
            margins: options.margins.clamped(),
            keys: options.keys,
            driver,
            renderer,
            software,
            frame_size: (width, height),
            pointer: Vec::new(),
            last_offset: (0, 0),
            last_scale: 1.0,
            last_window: (width, height),
            rotation,
            shows: 0,
            grab: None,
            fingers_seen: false,
            touch_matrix: options.touch_matrix,
            raw_lifts: Vec::new(),
            recording_lifts: false,
        })
    }

    /// What draws the frames: the video driver, the renderer's name, and
    /// whether it draws in software, for a report.
    pub fn renderer(&self) -> (String, String, bool) {
        (self.driver.clone(), self.renderer.clone(), self.software)
    }

    /// Set the panel's touch frame right from now on, as a calibration
    /// found it or the settings say.
    pub fn set_touch_matrix(&mut self, matrix: [f32; 6]) {
        self.touch_matrix = matrix;
    }

    /// The window's own size at the last show, in its own pixels.
    pub fn window_size(&self) -> (u32, u32) {
        self.last_window
    }

    /// Where fingers lifted since the last take, as raw shares of the panel
    /// before the matrix, for a calibration.
    pub fn take_raw_lifts(&mut self) -> Vec<(f32, f32)> {
        std::mem::take(&mut self.raw_lifts)
    }

    /// Start or stop keeping lifts for a calibration. Starting drops any
    /// lift kept before, so only touches made at the targets count.
    pub fn record_lifts(&mut self, on: bool) {
        self.raw_lifts.clear();
        self.recording_lifts = on;
    }

    /// The share of the panel a finger must report, through the identity,
    /// to land on a point of the picture as it is shown now: the picture's
    /// point placed and turned as the window has it. What a calibration
    /// pairs with the raw share it read.
    pub fn expected_share(&self, picture: (i32, i32)) -> (f32, f32) {
        let (x, y) = (
            self.last_offset.0 as f32 + picture.0 as f32 * self.last_scale,
            self.last_offset.1 as f32 + picture.1 as f32 * self.last_scale,
        );
        let (px, py) = rotated_center((x, y), self.rotation, self.last_window);
        (
            px / self.last_window.0.max(1) as f32,
            py / self.last_window.1.max(1) as f32,
        )
    }

    /// Write the window's pixels, as shown, to a PNG at the given show
    /// (counted from now), once: the picture on a screen with no X server
    /// can be looked at from a terminal.
    pub fn grab_after(&mut self, shows: u32, path: std::path::PathBuf) {
        self.grab = Some((path, self.shows.wrapping_add(shows.max(1))));
    }

    /// Name the window.
    pub fn set_title(&mut self, title: &str) {
        let _ = self.canvas.window_mut().set_title(title);
    }

    /// Put the frame's top left at a fixed point instead of centring it.
    pub fn place_at(&mut self, x: i32, y: i32) {
        self.placement = Some((x, y));
    }

    /// The picture under a frame that leaves part of the window bare, made
    /// at the size `screen_size` gives; none for the black. It stands
    /// until the next call.
    pub fn set_backdrop(&mut self, picture: Option<&Frame>) -> Result<(), String> {
        if let Some(old) = self.backdrop.take() {
            // With `unsafe_textures` a texture is freed by hand.
            unsafe { old.destroy() };
        }
        let Some(picture) = picture else {
            return Ok(());
        };
        let (width, height) = (picture.width.max(1), picture.height.max(1));
        let mut texture = self
            .creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, width, height)
            .map_err(|err| err.to_string())?;
        texture
            .update(None, &picture.rgba, picture.width as usize * 4)
            .map_err(|err| err.to_string())?;
        self.backdrop = Some(texture);
        Ok(())
    }

    /// The window's size in the picture's own orientation, a quarter turn
    /// swapping the two: what a backdrop is made at.
    pub fn screen_size(&self) -> (u32, u32) {
        let (w, h) = self.canvas.output_size().unwrap_or(self.frame_size);
        if matches!(self.rotation, 90 | 270) {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// Switch the panel: through its backlight where the kernel lists one;
    /// else through the connector's power on the card the window draws on
    /// (DPMS), or X's screen saver where an X server draws; where none of
    /// them can, nothing but the black frame the display shows. What it
    /// did comes back, for the player to hear.
    pub fn set_power(&mut self, on: bool) -> Power {
        let by = if self
            .backlight
            .as_deref()
            .is_some_and(|dir| backlight::set_power(dir, on).is_ok())
        {
            Means::Backlight
        } else if self.dpms(on) {
            Means::Dpms
        } else {
            Means::Dark
        };
        self.power = Power { on, by: Some(by) };
        self.power
    }

    /// The panel's power as last switched.
    pub fn power(&self) -> Power {
        self.power
    }

    /// The backlight's brightness and its top, where there is one.
    pub fn brightness(&self) -> Option<(u32, u32)> {
        backlight::brightness(self.backlight.as_deref()?)
    }

    /// The backlight set to `level`, held to its top: what it stands at
    /// then, or none without a backlight.
    pub fn set_brightness(&mut self, level: u32) -> Option<(u32, u32)> {
        backlight::set_brightness(self.backlight.as_deref()?, level).ok()
    }

    /// The connector's power through the card's own handle, which SDL
    /// holds when it draws on the screen itself; through X's screen saver
    /// when an X server draws. False where neither is there or the card
    /// refuses.
    fn dpms(&self, on: bool) -> bool {
        let driver = self.driver.to_ascii_lowercase();
        if driver == "kmsdrm" {
            #[cfg(target_os = "linux")]
            if let Some(fd) = self.drm_fd() {
                return match dpms::set(fd, on) {
                    Ok(()) => true,
                    Err(err) => {
                        eprintln!("glass: screen: {err}");
                        false
                    }
                };
            }
            return false;
        }
        if driver == "x11" {
            // Through the C library's own call, with the window's DISPLAY
            // in the environment: Rust's own spawn would ask the player's
            // glibc for a symbol it does not have.
            #[cfg(target_os = "linux")]
            {
                let command = if on {
                    c"xset dpms force on"
                } else {
                    c"xset dpms force off"
                };
                return unsafe { libc::system(command.as_ptr()) } == 0;
            }
            #[cfg(not(target_os = "linux"))]
            return false;
        }
        false
    }

    /// The card's handle SDL opened for the screen, from the window's own
    /// information. The KMSDRM member of that union is not named by these
    /// bindings; its layout is SDL's own: the card's index, then the handle.
    #[cfg(target_os = "linux")]
    fn drm_fd(&self) -> Option<std::os::fd::RawFd> {
        use sdl2::sys;
        let mut info: sys::SDL_SysWMinfo = unsafe { std::mem::zeroed() };
        let linked = sdl2::version::version();
        info.version = sys::SDL_version {
            major: linked.major,
            minor: linked.minor,
            patch: linked.patch,
        };
        let got = unsafe { sys::SDL_GetWindowWMInfo(self.canvas.window().raw(), &mut info) };
        if got != sys::SDL_bool::SDL_TRUE || info.subsystem != sys::SDL_SYSWM_TYPE::SDL_SYSWM_KMSDRM
        {
            return None;
        }
        let bytes = unsafe { info.info.dummy };
        let fd = i32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        (fd >= 0).then_some(fd)
    }

    /// Follow a new frame size: a window sized to the frame is resized; a
    /// fitted frame is scaled to the window at the next show.
    pub fn fit_to(&mut self, width: u32, height: u32) -> Result<(), String> {
        let (width, height) = (width.max(1), height.max(1));
        if self.frame_size == (width, height) {
            return Ok(());
        }
        self.frame_size = (width, height);
        if !self.fit {
            let (made_w, made_h) = scaled_size((width, height), self.scale);
            let window = self.canvas.window_mut();
            if (window.window_flags()
                & sdl2::sys::SDL_WindowFlags::SDL_WINDOW_FULLSCREEN_DESKTOP as u32)
                == 0
            {
                window
                    .set_size(made_w, made_h)
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    /// The window's events since the last call: fingers and mouse buttons
    /// as pointer events kept for `take_pointer`, the keys that close or
    /// resize the window as what the caller does next; `touched` when a
    /// finger or a button was lifted on the window.
    fn poll_events(&mut self) -> Result<(bool, Option<Shown>), String> {
        let mut touched = false;
        let mut raw: Vec<(PointerKind, i32, i32)> = Vec::new();
        for event in self.pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok((touched, Some(Shown::Closed))),
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } if self.keys => return Ok((touched, Some(Shown::Closed))),
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } if self.keys => return Ok((touched, Some(Shown::LeaveFullscreen))),
                Event::KeyDown {
                    keycode: Some(Keycode::F),
                    ..
                } if self.keys => return Ok((touched, Some(Shown::EnterFullscreen))),
                // A finger arrives as a finger, with its place as a share of
                // the window, and SDL may make a mouse of it as well; once a
                // finger has been seen, those made-up mouse events are
                // dropped, so a touch counts once. A real mouse counts always.
                Event::FingerDown { x, y, .. } => {
                    self.fingers_seen = true;
                    let (px, py) =
                        finger_pixel(through(self.touch_matrix, (x, y)), self.last_window);
                    raw.push((PointerKind::Down, px, py));
                }
                Event::FingerMotion { x, y, .. } => {
                    let (px, py) =
                        finger_pixel(through(self.touch_matrix, (x, y)), self.last_window);
                    raw.push((PointerKind::Move, px, py));
                }
                Event::FingerUp { x, y, .. } => {
                    if self.recording_lifts && self.raw_lifts.len() < 64 {
                        self.raw_lifts.push((x, y));
                    }
                    let (px, py) =
                        finger_pixel(through(self.touch_matrix, (x, y)), self.last_window);
                    raw.push((PointerKind::Up, px, py));
                    touched = true;
                }
                Event::MouseButtonDown { which, x, y, .. }
                    if keep_mouse(which, self.fingers_seen) =>
                {
                    raw.push((PointerKind::Down, x, y))
                }
                Event::MouseButtonUp { which, x, y, .. }
                    if keep_mouse(which, self.fingers_seen) =>
                {
                    raw.push((PointerKind::Up, x, y));
                    touched = true;
                }
                Event::MouseMotion {
                    which,
                    x,
                    y,
                    mousestate,
                    ..
                } if mousestate.left() && keep_mouse(which, self.fingers_seen) => {
                    raw.push((PointerKind::Move, x, y))
                }
                _ => {}
            }
        }
        for (kind, x, y) in raw {
            self.push_pointer(kind, x, y);
        }
        Ok((touched, None))
    }

    /// Pump the window's events without showing a frame, while the panel
    /// is off: what the window saw, as `show` would say it.
    pub fn pump(&mut self) -> Result<Shown, String> {
        let (touched, closing) = self.poll_events()?;
        Ok(closing.unwrap_or(if touched { Shown::Touched } else { Shown::Kept }))
    }

    /// Upload one frame, or only its `changed` boxes when the texture already
    /// holds the rest, and say what the window saw meanwhile.
    pub fn show(&mut self, frame: &Frame, changed: &[Rect]) -> Result<Shown, String> {
        let (touched, closing) = self.poll_events()?;
        if let Some(shown) = closing {
            return Ok(shown);
        }
        let fits = self.texture.as_ref().is_some_and(|t| {
            let q = t.query();
            q.width == frame.width && q.height == frame.height
        });
        if !fits {
            if let Some(old) = self.texture.take() {
                // With `unsafe_textures` a texture is freed by hand.
                unsafe { old.destroy() };
            }
            let texture = self
                .creator
                .create_texture_streaming(PixelFormatEnum::RGBA32, frame.width, frame.height)
                .map_err(|err| err.to_string())?;
            self.texture = Some(texture);
        }
        let texture = self.texture.as_mut().expect("texture was just made");
        let pitch = frame.width as usize * 4;
        let whole = !fits
            || changed
                .iter()
                .any(|r| r.x == 0 && r.y == 0 && r.w >= frame.width && r.h >= frame.height);
        if whole {
            texture
                .update(None, &frame.rgba, pitch)
                .map_err(|err| err.to_string())?;
        } else {
            if changed.is_empty() {
                // The window shows this frame already.
                return Ok(if touched { Shown::Touched } else { Shown::Kept });
            }
            for r in changed {
                let from = (r.y as usize * frame.width as usize + r.x as usize) * 4;
                let rect = sdl2::rect::Rect::new(r.x as i32, r.y as i32, r.w.max(1), r.h.max(1));
                texture
                    .update(Some(rect), &frame.rgba[from..], pitch)
                    .map_err(|err| err.to_string())?;
            }
        }
        self.canvas
            .set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        self.canvas.clear();
        let (window_w, window_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        // The picture is placed in its own orientation; a quarter turn
        // means the window is as tall as the picture is wide.
        let (logical_w, logical_h) = if matches!(self.rotation, 90 | 270) {
            (window_h, window_w)
        } else {
            (window_w, window_h)
        };
        let (x, y, w, h, scale) = if self.fit {
            fitted_rect(
                (frame.width, frame.height),
                (logical_w, logical_h),
                self.placement,
            )
        } else {
            placed_rect_within(
                (frame.width, frame.height),
                (logical_w, logical_h),
                self.placement,
                self.scale,
                self.anchor,
                self.margins,
            )
        };
        self.last_offset = (x, y);
        self.last_scale = scale;
        self.last_window = (window_w, window_h);
        // The backdrop under a frame that leaves part of the window bare:
        // the whole window, turned as the frame is. A frame that covers the
        // window has nothing under it to show.
        if let Some(backdrop) = self.backdrop.as_ref() {
            let bare = x > 0
                || y > 0
                || x + (w as i32) < (logical_w as i32)
                || y + (h as i32) < (logical_h as i32);
            if bare {
                if self.rotation == 0 {
                    self.canvas
                        .copy(backdrop, None, None)
                        .map_err(|err| err.to_string())?;
                } else {
                    let turned = sdl2::rect::Rect::from_center(
                        (window_w as i32 / 2, window_h as i32 / 2),
                        logical_w.max(1),
                        logical_h.max(1),
                    );
                    self.canvas
                        .copy_ex(
                            backdrop,
                            None,
                            turned,
                            self.rotation as f64,
                            None,
                            false,
                            false,
                        )
                        .map_err(|err| err.to_string())?;
                }
            }
        }
        if self.rotation == 0 {
            let dest = sdl2::rect::Rect::new(x, y, w.max(1), h.max(1));
            self.canvas
                .copy(texture, None, dest)
                .map_err(|err| err.to_string())?;
        } else {
            // SDL turns the picture about the centre of the rectangle it is
            // given, so that centre goes where the turned picture's centre lands.
            let centre = rotated_center(
                (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0),
                self.rotation,
                (window_w, window_h),
            );
            let turned = sdl2::rect::Rect::from_center(
                (centre.0.round() as i32, centre.1.round() as i32),
                w.max(1),
                h.max(1),
            );
            self.canvas
                .copy_ex(
                    texture,
                    None,
                    turned,
                    self.rotation as f64,
                    None,
                    false,
                    false,
                )
                .map_err(|err| err.to_string())?;
        }
        self.shows = self.shows.wrapping_add(1);
        if self.grab.as_ref().is_some_and(|(_, at)| self.shows >= *at) {
            let (path, _) = self.grab.take().expect("the grab is set");
            match self.canvas.read_pixels(None, PixelFormatEnum::RGBA32) {
                Ok(rgba) => {
                    let shown = Frame {
                        blend: expose::Blend::Normal,
                        width: window_w,
                        height: window_h,
                        rgba,
                    };
                    match expose::write_png(&path, &shown) {
                        Ok(()) => println!("glass: grab {}", path.display()),
                        Err(err) => eprintln!("glass: grab: {err}"),
                    }
                }
                Err(err) => eprintln!("glass: grab: {err}"),
            }
        }
        self.canvas.present();
        Ok(if touched { Shown::Touched } else { Shown::Kept })
    }
}

/// Where a frame fitted to a window goes: scaled by one factor both ways
/// until it fills the window's width or height, whichever comes first,
/// centred, or with its top left at `placement`; and the scale it took.
pub fn fitted_rect(
    frame: (u32, u32),
    window: (u32, u32),
    placement: Option<(i32, i32)>,
) -> (i32, i32, u32, u32, f32) {
    let (fw, fh) = (frame.0.max(1) as f32, frame.1.max(1) as f32);
    let (ww, wh) = (window.0.max(1) as f32, window.1.max(1) as f32);
    let scale = (ww / fw).min(wh / fh);
    let w = (fw * scale).round().max(1.0) as u32;
    let h = (fh * scale).round().max(1.0) as u32;
    let (x, y) = placement.unwrap_or((
        (window.0 as i32 - w as i32) / 2,
        (window.1 as i32 - h as i32) / 2,
    ));
    (x, y, w, h, scale)
}

/// Where a point of the picture, in the picture's own orientation, lands
/// on a window of `window` pixels when the picture is turned clockwise by
/// `rotation` degrees (0, 90, 180 or 270): the top left of the picture
/// goes to the top right at 90 and to the bottom left at 270.
pub fn rotated_center(point: (f32, f32), rotation: u32, window: (u32, u32)) -> (f32, f32) {
    let (x, y) = point;
    let (w, h) = (window.0 as f32, window.1 as f32);
    match rotation {
        90 => (w - y, x),
        180 => (w - x, h - y),
        270 => (y, h - x),
        _ => (x, y),
    }
}

/// A share of the panel through the touch matrix.
pub fn through(m: [f32; 6], share: (f32, f32)) -> (f32, f32) {
    (
        m[0] * share.0 + m[1] * share.1 + m[2],
        m[3] * share.0 + m[4] * share.1 + m[5],
    )
}

/// A finger's place, a share of the window each way, as a window pixel.
pub fn finger_pixel(share: (f32, f32), window: (u32, u32)) -> (i32, i32) {
    (
        (share.0 * window.0 as f32).round() as i32,
        (share.1 * window.1 as f32).round() as i32,
    )
}

/// The mouse SDL makes up from touches, `SDL_TOUCH_MOUSEID`: all ones.
pub const TOUCH_MOUSEID: u32 = u32::MAX;

/// Whether a mouse event counts: always from a real mouse; from a touch
/// SDL made a mouse of, only until the first finger has arrived as a
/// finger, after which the finger events carry the touch.
pub fn keep_mouse(which: u32, fingers_seen: bool) -> bool {
    which != TOUCH_MOUSEID || !fingers_seen
}

/// A window pixel back to the picture's own pixel, the turn undone.
pub fn unrotate_point(point: (i32, i32), rotation: u32, window: (u32, u32)) -> (i32, i32) {
    let (px, py) = point;
    let (w, h) = (window.0 as i32, window.1 as i32);
    match rotation {
        90 => (py, w - 1 - px),
        180 => (w - 1 - px, h - 1 - py),
        270 => (h - 1 - py, px),
        _ => (px, py),
    }
}

/// Publish a scene to a remote glass. No pixels cross this call.
pub fn publish(scene: &Scene) {
    let _ = scene;
}

/// Write an RGB PPM. The alpha byte is dropped. Used to look at a frame with no display.
pub fn write_ppm(path: impl AsRef<Path>, frame: &Frame) -> io::Result<()> {
    // Written beside the target and renamed over it, so a reader never sees
    // a half-written frame when the file is rewritten every step.
    let path = path.as_ref();
    let part = path.with_extension("ppm.part");
    let mut file = File::create(&part)?;
    write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
    let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
    for px in frame.rgba.as_chunks::<4>().0 {
        rgb.extend_from_slice(&px[..3]);
    }
    file.write_all(&rgb)?;
    drop(file);
    std::fs::rename(&part, path)
}

#[cfg(test)]
mod tests {
    use super::{
        asked_for, finger_pixel, fitted_rect, keep_mouse, kms, placed_rect, placed_rect_within,
        rotated_center, scaled_size, shows_nothing, through, unrotate_point, Anchor, Margins,
        IDENTITY, MAX_MARGIN_PERCENT, TOUCH_MOUSEID,
    };

    fn connector(name: &str, status: &str) -> (String, String) {
        (name.to_string(), status.to_string())
    }

    #[test]
    fn the_card_is_the_one_with_a_connected_screen() {
        // A Raspberry Pi 5 with a DSI panel: the render card has no
        // connector, the DSI card has the panel, the HDMI card nothing.
        // Whatever order the system lists them in, the answer is the same.
        let pi = [
            connector("card2-HDMI-A-1", "disconnected\n"),
            connector("card2-HDMI-A-2", "disconnected\n"),
            connector("card1-DSI-2", "connected\n"),
            connector("version", ""),
        ];
        let turned: Vec<_> = pi.iter().rev().cloned().collect();
        for listed in [&pi[..], &turned[..]] {
            assert_eq!(kms::connected_card(listed).as_deref(), Some("card1"));
            assert_eq!(kms::pick_card(listed).as_deref(), Some("card1"));
        }
        assert_eq!(kms::card_index("card1"), Some(1));
        assert_eq!(kms::card_index("card12"), Some(12));
        assert_eq!(kms::card_index("renderD128"), None);
        // Nothing connected: no card for SDL, and for a look the first
        // card that has a connector at all.
        let bare = [
            connector("card1-HDMI-A-2", "disconnected"),
            connector("card1-HDMI-A-1", "disconnected"),
        ];
        assert_eq!(kms::connected_card(&bare), None);
        assert_eq!(kms::pick_card(&bare).as_deref(), Some("card1"));
        // No connector listed: the kernel drives no screen.
        assert_eq!(kms::pick_card(&[connector("card0", "")]), None);
        assert_eq!(kms::pick_card(&[]), None);
        // Two screens connected: the first by name, the same at every start.
        let two = [
            connector("card2-HDMI-A-1", "connected"),
            connector("card1-DSI-2", "connected"),
        ];
        assert_eq!(kms::connected_card(&two).as_deref(), Some("card1"));
    }

    #[test]
    fn a_driver_that_shows_nothing_is_taken_only_when_asked_for_by_name() {
        assert!(shows_nothing("offscreen") && shows_nothing("dummy"));
        assert!(!shows_nothing("x11") && !shows_nothing("KMSDRM") && !shows_nothing("wayland"));
        assert!(!asked_for("", "offscreen"), "SDL's own fallback");
        assert!(!asked_for("x11", "offscreen"));
        assert!(asked_for("offscreen", "offscreen"));
        assert!(asked_for("dummy", "dummy"));
        assert!(
            asked_for("x11, Offscreen", "offscreen"),
            "a list tried in order"
        );
    }

    /// A finger at 92% across and 96% down a 720x1280 panel is window pixel
    /// (662, 1229); a real mouse always counts, a mouse made from a touch
    /// only until a finger has been seen as one.
    #[test]
    fn a_finger_is_a_window_pixel_and_a_made_up_mouse_counts_once() {
        assert_eq!(finger_pixel((0.92, 0.96), (720, 1280)), (662, 1229));
        assert_eq!(finger_pixel((0.0, 1.0), (720, 1280)), (0, 1280));
        assert!(keep_mouse(0, false) && keep_mouse(0, true));
        assert!(keep_mouse(TOUCH_MOUSEID, false));
        assert!(!keep_mouse(TOUCH_MOUSEID, true));
        assert_eq!(through(IDENTITY, (0.3, 0.7)), (0.3, 0.7));
        assert_eq!(
            through([0.0, 1.0, 0.0, 1.0, 0.0, 0.0], (0.3, 0.7)),
            (0.7, 0.3),
            "a swap"
        );
        assert_eq!(
            through([-1.0, 0.0, 1.0, 0.0, 1.0, 0.0], (0.3, 0.7)),
            (0.7, 0.7),
            "x flipped"
        );
    }

    /// A landscape picture on a portrait panel turned 270, the picture's top
    /// on the left as X's "left" puts it: the centre stays in the middle, the
    /// top left lands at the bottom left, at 90 at the top right; and a
    /// finger on a window pixel maps back to the picture pixel under it.
    #[test]
    fn a_turned_picture_and_a_turned_finger_meet() {
        assert_eq!(
            rotated_center((640.0, 360.0), 270, (720, 1280)),
            (360.0, 640.0)
        );
        assert_eq!(rotated_center((0.0, 0.0), 270, (720, 1280)), (0.0, 1280.0));
        assert_eq!(rotated_center((0.0, 0.0), 90, (720, 1280)), (720.0, 0.0));
        assert_eq!(
            rotated_center((0.0, 0.0), 180, (1280, 720)),
            (1280.0, 720.0)
        );
        assert_eq!(rotated_center((5.0, 6.0), 0, (1280, 720)), (5.0, 6.0));
        assert_eq!(unrotate_point((662, 1224), 270, (720, 1280)), (55, 662));
        assert_eq!(unrotate_point((664, 55), 90, (720, 1280)), (55, 55));
        assert_eq!(unrotate_point((10, 20), 180, (1280, 720)), (1269, 699));
        assert_eq!(unrotate_point((10, 20), 0, (1280, 720)), (10, 20));
    }

    /// A 1024x600 frame on a 1280x720 screen fills the height at 1.2 and
    /// sits centred with a bar each side, or where a placement says; a
    /// tall frame fills the height and keeps its shape.
    #[test]
    fn a_frame_shown_smaller_keeps_its_shape_and_sits_where_the_anchor_says() {
        // At 100 and centred: the frame pixel for pixel in the middle, as before.
        assert_eq!(
            placed_rect((1024, 600), (1280, 720), None, 100, Anchor::Center),
            (128, 60, 1024, 600, 1.0)
        );
        // A placement given beats the anchor, as it does for a fitted frame.
        assert_eq!(
            placed_rect(
                (1024, 600),
                (1280, 720),
                Some((5, 7)),
                100,
                Anchor::BottomRight
            ),
            (5, 7, 1024, 600, 1.0)
        );
        // Three quarters, bottom right: the frame scaled both ways, against both edges.
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 75, Anchor::BottomRight),
            (480, 270, 1440, 810, 0.75)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::TopLeft),
            (0, 0, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::Top),
            (480, 0, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::Left),
            (0, 270, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::Right),
            (960, 270, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::Bottom),
            (480, 540, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::BottomLeft),
            (0, 540, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect((1920, 1080), (1920, 1080), None, 50, Anchor::TopRight),
            (960, 0, 960, 540, 0.5)
        );
        // The share is kept within its bounds: never larger than the frame, never below a tenth.
        assert_eq!(
            placed_rect((1000, 500), (4000, 2000), None, 250, Anchor::Center).2,
            1000
        );
        assert_eq!(scaled_size((1000, 500), 3), (100, 50));
        assert_eq!(scaled_size((1000, 500), 85), (850, 425));
        // A frame larger than the window at 100 sits past the window's edges, centred, as before.
        assert_eq!(
            placed_rect((1920, 1080), (1280, 720), None, 100, Anchor::Center),
            (-320, -180, 1920, 1080, 1.0)
        );
    }

    #[test]
    fn a_margin_keeps_a_frame_off_the_edges_it_sits_against_and_never_past_the_other() {
        let screen = (1920, 1080);
        let frame = (1920, 1080);
        // Three percent of the shorter side, 1080, is 32 pixels.
        let m = Margins::all(3);
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::TopLeft, m),
            (32, 32, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::BottomRight, m),
            (1920 - 960 - 32, 1080 - 540 - 32, 960, 540, 0.5)
        );
        // An edge the anchor does not touch keeps no margin; centred keeps none.
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::Top, m).0,
            480
        );
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::Center, m),
            placed_rect(frame, screen, None, 50, Anchor::Center)
        );
        // Each edge its own.
        let each = Margins {
            top: 10,
            right: 0,
            bottom: 0,
            left: 5,
        };
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::TopLeft, each),
            (54, 108, 960, 540, 0.5)
        );
        assert_eq!(
            placed_rect_within(frame, screen, None, 50, Anchor::BottomRight, each),
            (960, 540, 960, 540, 0.5)
        );
        // Never past the opposite edge: a frame as wide as the screen stays at 0,
        // and one with less spare than the margin keeps what spare there is.
        assert_eq!(
            placed_rect_within(frame, screen, None, 100, Anchor::TopLeft, Margins::all(10)),
            (0, 0, 1920, 1080, 1.0)
        );
        assert_eq!(
            placed_rect_within(
                (1800, 1000),
                screen,
                None,
                100,
                Anchor::BottomRight,
                Margins::all(10)
            ),
            (12, 0, 1800, 1000, 1.0)
        );
        // Larger than the screen: as without margins.
        assert_eq!(
            placed_rect_within((2400, 1080), screen, None, 100, Anchor::BottomRight, m),
            (-480, 0, 2400, 1080, 1.0)
        );
        // The share is bounded, and a placement given beats the margins.
        assert_eq!(Margins::all(90).clamped(), Margins::all(MAX_MARGIN_PERCENT));
        assert_eq!(
            placed_rect_within(frame, screen, Some((5, 7)), 50, Anchor::TopLeft, m).0,
            5
        );
    }

    #[test]
    fn a_fitted_frame_fills_one_way_and_keeps_its_shape() {
        assert_eq!(
            fitted_rect((1024, 600), (1280, 720), None),
            (25, 0, 1229, 720, 1.2)
        );
        assert_eq!(
            fitted_rect((1024, 600), (1280, 720), Some((0, 0))),
            (0, 0, 1229, 720, 1.2)
        );
        let (x, y, w, h, scale) = fitted_rect((600, 1024), (1280, 720), None);
        assert_eq!((y, h), (0, 720));
        assert!((scale - 720.0 / 1024.0).abs() < 1e-6 && w == 422 && x == (1280 - 422) / 2);
        assert_eq!(
            fitted_rect((1280, 720), (1280, 720), None),
            (0, 0, 1280, 720, 1.0)
        );
    }
}

/// By what means the panel's power is switched: its backlight's own file,
/// the connector's power on the card or X's screen saver (DPMS), or not
/// at all, the picture black instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Means {
    Backlight,
    Dpms,
    Dark,
}

impl Means {
    pub fn as_str(self) -> &'static str {
        match self {
            Means::Backlight => "backlight",
            Means::Dpms => "dpms",
            Means::Dark => "dark",
        }
    }
}

/// The panel's power as last switched, and by what means; none until the
/// surface has switched it once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Power {
    pub on: bool,
    pub by: Option<Means>,
}

impl Power {
    /// The means as the player hears it: empty until the first switch.
    pub fn by_name(self) -> &'static str {
        self.by.map_or("", Means::as_str)
    }
}

/// The panel's backlight as the kernel lists it under `/sys/class/backlight`:
/// its power and its brightness, by their files. The files are the
/// kernel's; where an image leaves them root's alone, every write fails
/// and the panel is switched by other means.
pub mod backlight {
    use std::path::{Path, PathBuf};

    const ROOT: &str = "/sys/class/backlight";

    /// The first backlight under `root`, by name, that has a power file.
    pub fn find_under(root: &Path) -> Option<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|dir| dir.join("bl_power").exists())
            .collect();
        dirs.sort();
        dirs.into_iter().next()
    }

    /// The panel's backlight, where the kernel lists one.
    pub fn find() -> Option<PathBuf> {
        find_under(Path::new(ROOT))
    }

    /// The power: 0 lights it, as the kernel's blanking numbers go; 1
    /// puts it out.
    pub fn set_power(dir: &Path, on: bool) -> Result<(), String> {
        let file = dir.join("bl_power");
        std::fs::write(&file, if on { "0" } else { "1" })
            .map_err(|err| format!("{}: {err}", file.display()))
    }

    /// Whether the backlight is lit, as its power file says.
    pub fn power(dir: &Path) -> Option<bool> {
        Some(read(&dir.join("bl_power"))? == 0)
    }

    /// The brightness and its top, as the kernel has them.
    pub fn brightness(dir: &Path) -> Option<(u32, u32)> {
        Some((
            read(&dir.join("brightness"))?,
            read(&dir.join("max_brightness"))?,
        ))
    }

    /// The brightness set to `level`, held to its top; what it stands at.
    pub fn set_brightness(dir: &Path, level: u32) -> Result<(u32, u32), String> {
        let (_, max) =
            brightness(dir).ok_or_else(|| format!("{}: no brightness", dir.display()))?;
        let level = level.min(max);
        let file = dir.join("brightness");
        std::fs::write(&file, level.to_string())
            .map_err(|err| format!("{}: {err}", file.display()))?;
        Ok((level, max))
    }

    fn read(path: &Path) -> Option<u32> {
        std::fs::read_to_string(path).ok()?.trim().parse().ok()
    }
}

/// The connector's power on a card through its handle: the DPMS property
/// the kernel keeps on each connector, set on every connector that has it.
#[cfg(target_os = "linux")]
mod dpms {
    use drm::control::Device as ControlDevice;
    use std::os::fd::{AsFd, BorrowedFd, RawFd};

    struct Card<'a>(BorrowedFd<'a>);

    impl AsFd for Card<'_> {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.0
        }
    }

    impl drm::Device for Card<'_> {}
    impl ControlDevice for Card<'_> {}

    /// DPMS on (0) or off (3), as the kernel numbers them, on every
    /// connector of the card that has the property.
    pub fn set(fd: RawFd, on: bool) -> Result<(), String> {
        // The handle is SDL's: borrowed for the calls, never closed here.
        let card = Card(unsafe { BorrowedFd::borrow_raw(fd) });
        let resources = card
            .resource_handles()
            .map_err(|err| format!("the card: {err}"))?;
        let mut set = 0;
        for connector in resources.connectors() {
            let Ok(props) = card.get_properties(*connector) else {
                continue;
            };
            let (ids, _) = props.as_props_and_values();
            for id in ids {
                let Ok(info) = card.get_property(*id) else {
                    continue;
                };
                if info.name().to_bytes() != b"DPMS" {
                    continue;
                }
                card.set_property(*connector, *id, if on { 0 } else { 3 })
                    .map_err(|err| format!("DPMS: {err}"))?;
                set += 1;
            }
        }
        if set == 0 {
            Err("no connector with a DPMS property".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod backlight_tests {
    use super::backlight;
    use std::path::PathBuf;

    /// A backlight as the kernel lists one, in a folder of this test's own.
    fn listed(name: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("glass-backlight-{}-{name}", std::process::id()));
        let dir = root.join("11-0045");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bl_power"), "0\n").unwrap();
        std::fs::write(dir.join("brightness"), "200\n").unwrap();
        std::fs::write(dir.join("max_brightness"), "255\n").unwrap();
        (root, dir)
    }

    /// The backlight found by its power file; its power put out and lit
    /// by the kernel's numbers; its brightness read, set and held to its top.
    #[test]
    fn the_backlight_is_found_switched_and_set() {
        let (root, dir) = listed("found");
        std::fs::create_dir_all(root.join("not-a-backlight")).unwrap();
        assert_eq!(backlight::find_under(&root), Some(dir.clone()));
        assert_eq!(backlight::power(&dir), Some(true));
        backlight::set_power(&dir, false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("bl_power")).unwrap(), "1");
        assert_eq!(backlight::power(&dir), Some(false));
        backlight::set_power(&dir, true).unwrap();
        assert_eq!(backlight::power(&dir), Some(true));
        assert_eq!(backlight::brightness(&dir), Some((200, 255)));
        assert_eq!(backlight::set_brightness(&dir, 300), Ok((255, 255)));
        assert_eq!(backlight::set_brightness(&dir, 40), Ok((40, 255)));
        assert_eq!(backlight::brightness(&dir), Some((40, 255)));
        let _ = std::fs::remove_dir_all(root);
    }

    /// No backlight listed: none found, and nothing to set.
    #[test]
    fn without_a_backlight_nothing_is_found() {
        let root =
            std::env::temp_dir().join(format!("glass-backlight-{}-none", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(backlight::find_under(&root), None);
        std::fs::create_dir_all(root.join("empty")).unwrap();
        assert_eq!(
            backlight::find_under(&root),
            None,
            "a folder without a power file is no backlight"
        );
        assert!(backlight::set_brightness(&root.join("empty"), 10).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
