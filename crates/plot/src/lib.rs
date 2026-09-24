//! Turn an [`lead::Input`] and a skin into a [`Scene`].
//! Pure: no files, no devices, no pixels.

use lead::{Input, SkinDesc};

/// What the surface should show. Angles, bar heights, and layers land here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub skin: String,
    pub has_levels: bool,
    pub bin_count: usize,
}

/// One step of the line. Headless mode may publish this and skip raster.
pub fn step(skin: &SkinDesc, input: &Input) -> Scene {
    Scene {
        skin: skin.name.clone(),
        has_levels: input.levels != lead::Levels::default(),
        bin_count: input.bins.values.len(),
    }
}
