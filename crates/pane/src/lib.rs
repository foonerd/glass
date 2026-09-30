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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowOptions {
    pub mode: WindowMode,
    /// Where the window goes in windowed and frameless modes; centred otherwise.
    pub position: Option<(i32, i32)>,
    /// Scale the frame to the window, keeping its shape, instead of
    /// showing it pixel for pixel: a theme larger or smaller than the screen fills it.
    pub fit: bool,
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
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            mode: WindowMode::Fullscreen,
            position: None,
            fit: false,
            pointer: false,
            keys: false,
            title: "Glass".to_string(),
            display: 0,
            driver: None,
            rotation: 0,
        }
    }
}

/// One window. The binary keeps it for the life of the player, with one
/// streaming texture the frames are uploaded into.
pub struct Surface {
    canvas: Canvas<Window>,
    creator: TextureCreator<WindowContext>,
    texture: Option<Texture>,
    pump: EventPump,
    /// Where the frame's top left goes; `None` centres it in the window.
    placement: Option<(i32, i32)>,
    fit: bool,
    keys: bool,
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
        let (width, height) = self.frame_size;
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
    /// and a fitted frame's scale.
    fn push_pointer(&mut self, kind: PointerKind, x: i32, y: i32) {
        let (x, y) = unrotate_point((x, y), self.rotation, self.last_window);
        let (x, y) = (x - self.last_offset.0, y - self.last_offset.1);
        let (x, y) = if self.fit && self.last_scale > 0.0 {
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
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let (width, height) = (width.max(1), height.max(1));
        let mut builder = video.window(&options.title, width, height);
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
        let creator = canvas.texture_creator();
        let pump = sdl.event_pump()?;
        sdl.mouse().show_cursor(options.pointer);
        Ok(Self {
            canvas,
            creator,
            texture: None,
            pump,
            placement: None,
            fit: options.fit,
            keys: options.keys,
            frame_size: (width, height),
            pointer: Vec::new(),
            last_offset: (0, 0),
            last_scale: 1.0,
            last_window: (width, height),
            rotation,
            shows: 0,
            grab: None,
            fingers_seen: false,
        })
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

    /// Follow a new frame size: a window sized to the frame is resized; a
    /// fitted frame is scaled to the window at the next show.
    pub fn fit_to(&mut self, width: u32, height: u32) -> Result<(), String> {
        let (width, height) = (width.max(1), height.max(1));
        if self.frame_size == (width, height) {
            return Ok(());
        }
        self.frame_size = (width, height);
        if !self.fit {
            let window = self.canvas.window_mut();
            if (window.window_flags()
                & sdl2::sys::SDL_WindowFlags::SDL_WINDOW_FULLSCREEN_DESKTOP as u32)
                == 0
            {
                window
                    .set_size(width, height)
                    .map_err(|err| err.to_string())?;
            }
        }
        Ok(())
    }

    /// Upload one frame, or only its `changed` boxes when the texture already
    /// holds the rest, and say what the window saw meanwhile.
    pub fn show(&mut self, frame: &Frame, changed: &[Rect]) -> Result<Shown, String> {
        let mut touched = false;
        let mut raw: Vec<(PointerKind, i32, i32)> = Vec::new();
        for event in self.pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(Shown::Closed),
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } if self.keys => return Ok(Shown::Closed),
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } if self.keys => return Ok(Shown::LeaveFullscreen),
                Event::KeyDown {
                    keycode: Some(Keycode::F),
                    ..
                } if self.keys => return Ok(Shown::EnterFullscreen),
                // A finger arrives as a finger, with its place as a share of
                // the window, and SDL may make a mouse of it as well; once a
                // finger has been seen, those made-up mouse events are
                // dropped, so a touch counts once. A real mouse counts always.
                Event::FingerDown { x, y, .. } => {
                    self.fingers_seen = true;
                    let (px, py) = finger_pixel((x, y), self.last_window);
                    raw.push((PointerKind::Down, px, py));
                }
                Event::FingerMotion { x, y, .. } => {
                    let (px, py) = finger_pixel((x, y), self.last_window);
                    raw.push((PointerKind::Move, px, py));
                }
                Event::FingerUp { x, y, .. } => {
                    let (px, py) = finger_pixel((x, y), self.last_window);
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
            let (x, y) = self.placement.unwrap_or((
                (logical_w as i32 - frame.width as i32) / 2,
                (logical_h as i32 - frame.height as i32) / 2,
            ));
            (x, y, frame.width, frame.height, 1.0)
        };
        self.last_offset = (x, y);
        self.last_scale = scale;
        self.last_window = (window_w, window_h);
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
        finger_pixel, fitted_rect, keep_mouse, rotated_center, unrotate_point, TOUCH_MOUSEID,
    };

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
