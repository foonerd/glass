//! Turn an [`lead::Input`] and a skin into a [`Scene`].
//! Pure: no files, no devices, no pixels.

use lead::{Input, SkinDesc};
use serde::{Deserialize, Serialize};

/// What the surface should show. Levels and bars are fractions from 0 to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub skin: String,
    pub width: u32,
    pub height: u32,
    pub left: f32,
    pub right: f32,
    pub bars: Vec<f32>,
    pub left_at: Option<(u32, u32)>,
    pub right_at: Option<(u32, u32)>,
    pub needle: Option<(f32, f32, f32)>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            skin: String::new(),
            width: 0,
            height: 0,
            left: 0.0,
            right: 0.0,
            bars: Vec::new(),
            left_at: None,
            right_at: None,
            needle: None,
        }
    }
}

/// One step of the line. Headless mode may publish this and skip raster.
pub fn step(skin: &SkinDesc, input: &Input) -> Scene {
    let meter_max = skin.meter_max.max(1.0);
    let spectrum_max = skin.spectrum_max.max(1.0);
    Scene {
        skin: skin.name.clone(),
        width: skin.width.max(1),
        height: skin.height.max(1),
        left: (input.levels.left / meter_max).clamp(0.0, 1.0),
        right: (input.levels.right / meter_max).clamp(0.0, 1.0),
        bars: input
            .bins
            .values
            .iter()
            .map(|bin| (bin / spectrum_max).clamp(0.0, 1.0))
            .collect(),
        left_at: skin.left_at,
        right_at: skin.right_at,
        needle: skin.needle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lead::{Bins, Input, Levels, SkinDesc};

    #[test]
    fn half_scale_levels_and_a_full_bar() {
        let skin = SkinDesc::basic();
        let input = Input {
            levels: Levels {
                left: 50.0,
                right: 0.0,
                mono: 25.0,
            },
            bins: Bins {
                values: vec![100.0, 0.0],
            },
            metadata: lead::Metadata::default(),
        };
        let scene = step(&skin, &input);
        assert_eq!(scene.left, 0.5);
        assert_eq!(scene.right, 0.0);
        assert_eq!(scene.bars, vec![1.0, 0.0]);
        assert_eq!((scene.width, scene.height), (800, 480));
    }

    /// One recorded step: the skin and input that went in, the scene that came out.
    /// `glass --record` writes these under `testdata/frames/`.
    #[derive(Deserialize)]
    struct Recorded {
        skin: SkinDesc,
        input: Input,
        scene: Scene,
    }

    #[test]
    fn recorded_frames_replay() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/frames");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        for path in &paths {
            let text = std::fs::read_to_string(path).unwrap();
            let recorded: Recorded = serde_json::from_str(&text)
                .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            assert_eq!(
                step(&recorded.skin, &recorded.input),
                recorded.scene,
                "{}",
                path.display()
            );
        }
        println!("recorded frames replayed: {}", paths.len());
    }
}
