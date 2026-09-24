//! Put a finished frame on a surface, or send a scene to another glass.
//! This station does not decide angles or bar heights.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use expose::Frame;
use plot::Scene;
use sdl2::event::Event;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::EventPump;

/// One window. The binary keeps it for the life of the player.
pub struct Surface {
    canvas: Canvas<Window>,
    pump: EventPump,
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
        let pump = sdl.event_pump()?;
        Ok(Self { canvas, pump })
    }

    /// Upload one frame. `false` means the window was closed.
    pub fn show(&mut self, frame: &Frame) -> Result<bool, String> {
        for event in self.pump.poll_iter() {
            if let Event::Quit { .. } = event {
                return Ok(false);
            }
        }
        let creator = self.canvas.texture_creator();
        let mut texture = creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, frame.width, frame.height)
            .map_err(|err| err.to_string())?;
        texture
            .with_lock(None, |buffer, pitch| {
                let row = frame.width as usize * 4;
                for y in 0..frame.height as usize {
                    let src = y * row;
                    let dst = y * pitch;
                    buffer[dst..dst + row].copy_from_slice(&frame.rgba[src..src + row]);
                }
            })
            .map_err(|err| err.to_string())?;
        self.canvas
            .copy(&texture, None, None)
            .map_err(|err| err.to_string())?;
        self.canvas.present();
        Ok(true)
    }
}

/// Publish a scene to a remote glass. No pixels cross this call.
pub fn publish(scene: &Scene) {
    let _ = scene;
}

/// Write an RGB PPM. The alpha byte is dropped. Used to look at a frame with no display.
pub fn write_ppm(path: impl AsRef<Path>, frame: &Frame) -> io::Result<()> {
    let mut file = File::create(path)?;
    write!(file, "P6\n{} {}\n255\n", frame.width, frame.height)?;
    let mut rgb = Vec::with_capacity((frame.width * frame.height * 3) as usize);
    for px in frame.rgba.chunks_exact(4) {
        rgb.extend_from_slice(&px[..3]);
    }
    file.write_all(&rgb)
}
