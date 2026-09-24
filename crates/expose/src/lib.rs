//! Raster a [`plot::Scene`] into one RGBA frame.
//! This station does not poll a FIFO or talk to the player.

use plot::Scene;

/// One finished picture. `pane` uploads it once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        }
    }
}

/// Build the frame. An empty scene yields an empty frame until assets exist.
pub fn raster(scene: &Scene) -> Frame {
    let _ = scene;
    Frame::default()
}
