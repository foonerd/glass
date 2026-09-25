//! Put a finished frame on a surface, or send a scene to another glass.
//! This station does not decide angles or bar heights.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use expose::{Frame, Rect};
use plot::Scene;
use sdl2::event::Event;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
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
}

impl Surface {
    /// Open a window on the current display. Fails when SDL cannot start.
    pub fn open(width: u32, height: u32) -> Result<Self, String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video
            .window("Glass", width.max(1), height.max(1))
            .position_centered()
            .fullscreen_desktop()
            .borderless()
            .build()
            .map_err(|err| err.to_string())?;
        let canvas = window.into_canvas().build().map_err(|err| err.to_string())?;
        // Say what draws the frames, so a report from a player tells whether
        // the upload goes through hardware.
        let info = canvas.info();
        println!("glass: renderer {} on {}", info.name, video.current_video_driver());
        let creator = canvas.texture_creator();
        let pump = sdl.event_pump()?;
        // A player's glass shows no pointer.
        sdl.mouse().show_cursor(false);
        Ok(Self { canvas, creator, texture: None, pump, placement: None })
    }

    /// Put the frame's top left at a fixed point instead of centring it.
    pub fn place_at(&mut self, x: i32, y: i32) {
        self.placement = Some((x, y));
    }

    /// Upload one frame, or only its `changed` boxes when the texture already
    /// holds the rest, and say what the window saw meanwhile.
    pub fn show(&mut self, frame: &Frame, changed: &[Rect]) -> Result<Shown, String> {
        let mut touched = false;
        for event in self.pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(Shown::Closed),
                Event::MouseButtonUp { .. } | Event::FingerUp { .. } => touched = true,
                _ => {}
            }
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
        let whole = !fits || changed.iter().any(|r| r.x == 0 && r.y == 0 && r.w >= frame.width && r.h >= frame.height);
        if whole {
            texture.update(None, &frame.rgba, pitch).map_err(|err| err.to_string())?;
        } else {
            if changed.is_empty() {
                // The window shows this frame already.
                return Ok(if touched { Shown::Touched } else { Shown::Kept });
            }
            for r in changed {
                let from = (r.y as usize * frame.width as usize + r.x as usize) * 4;
                let rect = sdl2::rect::Rect::new(r.x as i32, r.y as i32, r.w.max(1), r.h.max(1));
                texture.update(Some(rect), &frame.rgba[from..], pitch).map_err(|err| err.to_string())?;
            }
        }
        self.canvas.set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        self.canvas.clear();
        let (window_w, window_h) = self.canvas.output_size().unwrap_or((frame.width, frame.height));
        let (x, y) = self.placement.unwrap_or(((window_w as i32 - frame.width as i32) / 2, (window_h as i32 - frame.height as i32) / 2));
        let dest = sdl2::rect::Rect::new(x, y, frame.width, frame.height);
        self.canvas.copy(texture, None, dest).map_err(|err| err.to_string())?;
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
    for px in frame.rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
    }
    file.write_all(&rgb)?;
    drop(file);
    std::fs::rename(&part, path)
}
