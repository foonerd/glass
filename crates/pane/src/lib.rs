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
    /// Where the frame's top left sat in the window at the last show,
    /// for mapping window pixels to frame pixels when not fitted.
    last_offset: (i32, i32),
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

    /// Keep a pointer event in frame pixels: a fitted window's events are
    /// already scaled to the frame; otherwise the frame's offset comes off.
    fn push_pointer(&mut self, kind: PointerKind, x: i32, y: i32) {
        let (x, y) = if self.fit {
            (x, y)
        } else {
            (x - self.last_offset.0, y - self.last_offset.1)
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
        let mut canvas = window
            .into_canvas()
            .build()
            .map_err(|err| err.to_string())?;
        if options.fit {
            canvas
                .set_logical_size(width, height)
                .map_err(|err| err.to_string())?;
        }
        // Say what draws the frames, so a report from a player tells whether
        // the upload goes through hardware.
        let info = canvas.info();
        println!(
            "glass: renderer {} on {}",
            info.name,
            video.current_video_driver()
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
        })
    }

    /// Name the window.
    pub fn set_title(&mut self, title: &str) {
        let _ = self.canvas.window_mut().set_title(title);
    }

    /// Put the frame's top left at a fixed point instead of centring it.
    pub fn place_at(&mut self, x: i32, y: i32) {
        self.placement = Some((x, y));
    }

    /// Follow a new frame size: a window sized to the frame is resized, a
    /// fitted one maps the new size to the screen.
    pub fn fit_to(&mut self, width: u32, height: u32) -> Result<(), String> {
        let (width, height) = (width.max(1), height.max(1));
        if self.frame_size == (width, height) {
            return Ok(());
        }
        self.frame_size = (width, height);
        if self.fit {
            self.canvas
                .set_logical_size(width, height)
                .map_err(|err| err.to_string())?;
        } else {
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
                // A finger arrives as a mouse too; the mouse events carry the
                // position, scaled to the frame when it is fitted.
                Event::MouseButtonDown { x, y, .. } => raw.push((PointerKind::Down, x, y)),
                Event::MouseButtonUp { x, y, .. } => {
                    raw.push((PointerKind::Up, x, y));
                    touched = true;
                }
                Event::MouseMotion {
                    x, y, mousestate, ..
                } if mousestate.left() => raw.push((PointerKind::Move, x, y)),
                Event::FingerUp { .. } => touched = true,
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
        if self.fit {
            // The logical size is the frame's: the renderer scales it to the window.
            self.canvas
                .copy(texture, None, None)
                .map_err(|err| err.to_string())?;
        } else {
            let (window_w, window_h) = self
                .canvas
                .output_size()
                .unwrap_or((frame.width, frame.height));
            let (x, y) = self.placement.unwrap_or((
                (window_w as i32 - frame.width as i32) / 2,
                (window_h as i32 - frame.height as i32) / 2,
            ));
            self.last_offset = (x, y);
            let dest = sdl2::rect::Rect::new(x, y, frame.width, frame.height);
            self.canvas
                .copy(texture, None, dest)
                .map_err(|err| err.to_string())?;
        }
        self.canvas.present();
        Ok(if touched { Shown::Touched } else { Shown::Kept })
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
