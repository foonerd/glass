//! Put a finished frame on a surface, or send a scene to another glass.
//! This station does not decide angles or bar heights.

use expose::Frame;
use plot::Scene;

/// Show one frame. The SDL upload lands here later.
pub fn show(frame: &Frame) {
    let _ = frame;
}

/// Publish a scene to a remote glass. No pixels cross this call.
pub fn publish(scene: &Scene) {
    let _ = scene;
}
