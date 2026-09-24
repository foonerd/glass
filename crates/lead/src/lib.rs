//! FIFO bytes, UDP packets, and skin files become these types.
//! This station does not open devices or draw.

/// Left, right, and mono levels, already scaled for the scene.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    pub left: f32,
    pub right: f32,
    pub mono: f32,
}

/// Latest spectrum frame. Older frames are discarded before they arrive here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bins {
    pub values: Vec<f32>,
}

/// Now-playing text for the surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

/// One snapshot of the outside world. `plot` turns it into a scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Input {
    pub levels: Levels,
    pub bins: Bins,
    pub metadata: Metadata,
}

/// Geometry and asset names read from a skin. Pixels stay in `expose`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkinDesc {
    pub name: String,
}
