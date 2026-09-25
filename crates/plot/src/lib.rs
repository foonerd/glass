//! Turn an [`lead::Input`] and a skin into a [`Scene`].
//! Pure: no files, no devices, no pixels.

use lead::{Input, Metadata, SkinDesc, TextSpec, TextStyle};
use serde::{Deserialize, Serialize};

/// One text the surface shows, already composed and coloured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub x: u32,
    pub y: u32,
    pub style: TextStyle,
    pub size: u32,
    pub color: [u8; 3],
    pub max_width: u32,
    pub text: String,
}

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
    #[serde(default)]
    pub texts: Vec<Text>,
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
            texts: Vec::new(),
        }
    }
}

/// Final ten seconds of a track, as the player colours them.
const LAST_SECONDS: [u8; 3] = [242, 0, 0];

fn text(spec: &TextSpec, content: String, color: [u8; 3]) -> Text {
    Text {
        x: spec.x,
        y: spec.y,
        style: spec.style,
        size: spec.size,
        color,
        max_width: spec.max_width,
        text: content,
    }
}

/// Whole seconds left in the track, or `None` when the source has no length.
pub fn seconds_remaining(meta: &Metadata) -> Option<u32> {
    if meta.duration <= 0.0 {
        return None;
    }
    Some((meta.duration - meta.seek).max(0.0).floor() as u32)
}

/// The texts a skin shows for this metadata. Artist and album share one line
/// unless the skin places the album on its own.
pub fn texts(skin: &SkinDesc, meta: &Metadata) -> Vec<Text> {
    let mut out = Vec::new();
    if let Some(spec) = &skin.title {
        if !meta.title.is_empty() {
            out.push(text(spec, meta.title.clone(), spec.color));
        }
    }
    if let Some(spec) = &skin.artist {
        let line = if skin.album.is_none() && !meta.album.is_empty() {
            if meta.artist.is_empty() {
                meta.album.clone()
            } else {
                format!("{} - {}", meta.artist, meta.album)
            }
        } else {
            meta.artist.clone()
        };
        if !line.is_empty() {
            out.push(text(spec, line, spec.color));
        }
    }
    if let Some(spec) = &skin.album {
        if !meta.album.is_empty() {
            out.push(text(spec, meta.album.clone(), spec.color));
        }
    }
    if let Some(spec) = &skin.sample {
        let line = format!("{} {}", meta.samplerate, meta.bitdepth);
        let line = line.trim();
        if !line.is_empty() {
            out.push(text(spec, line.to_string(), spec.color));
        }
    }
    if let Some(spec) = &skin.time {
        if let Some(left) = seconds_remaining(meta) {
            let color = if (1..=10).contains(&left) {
                LAST_SECONDS
            } else {
                spec.color
            };
            out.push(text(spec, format!("{:02}:{:02}", left / 60, left % 60), color));
        }
    }
    out
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
        texts: texts(skin, &input.metadata),
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

    fn spec(style: TextStyle, color: [u8; 3]) -> TextSpec {
        TextSpec {
            x: 1,
            y: 2,
            style,
            size: 20,
            color,
            max_width: 0,
        }
    }

    #[test]
    fn texts_join_artist_and_album_and_count_down() {
        let mut skin = SkinDesc::basic();
        skin.title = Some(spec(TextStyle::Bold, [255, 237, 76]));
        skin.artist = Some(spec(TextStyle::Light, [255, 255, 255]));
        skin.sample = Some(spec(TextStyle::Regular, [255, 255, 255]));
        skin.time = Some(spec(TextStyle::Digi, [180, 180, 180]));
        let meta = Metadata {
            title: "Wonder".into(),
            artist: "Courtney Barnett".into(),
            album: "Creature of Habit".into(),
            samplerate: "44.1 kHz".into(),
            bitdepth: "16-bit".into(),
            status: "play".into(),
            duration: 218.051,
            seek: 1.995,
        };
        let lines: Vec<String> = texts(&skin, &meta).into_iter().map(|t| t.text).collect();
        assert_eq!(lines, ["Wonder", "Courtney Barnett - Creature of Habit", "44.1 kHz 16-bit", "03:36"]);

        skin.album = Some(spec(TextStyle::Light, [0, 0, 0]));
        let lines: Vec<String> = texts(&skin, &meta).into_iter().map(|t| t.text).collect();
        assert_eq!(lines[1], "Courtney Barnett");
        assert_eq!(lines[2], "Creature of Habit");

        let ending = Metadata { seek: 210.0, ..meta.clone() };
        let clock = texts(&skin, &ending).pop().unwrap();
        assert_eq!((clock.text.as_str(), clock.color), ("00:08", LAST_SECONDS));

        let stream = Metadata { duration: 0.0, ..meta };
        assert!(texts(&skin, &stream).iter().all(|t| t.style != TextStyle::Digi));
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
