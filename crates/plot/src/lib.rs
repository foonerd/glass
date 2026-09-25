//! Turn an [`lead::Input`] and a skin into a [`Scene`].
//! Pure: no files, no devices, no pixels.

use lead::{
    format_key, format_label, Input, Metadata, ScrollDirection, SkinDesc, TextAlign, TextSpec,
    TextStyle, TypeAlign, TypeMode,
};
use serde::{Deserialize, Serialize};

/// The type area to show: the box from the skin, the label for the track
/// type, and the icon file when one exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeArea {
    pub x: u32,
    pub y: u32,
    pub box_size: Option<(u32, u32)>,
    pub mode: TypeMode,
    pub align: TypeAlign,
    pub color: [u8; 3],
    pub font_size: u32,
    pub font_style: TextStyle,
    pub label: String,
    /// Icon file, or empty. A `.svg` is tinted with `color`; a `.png` is not.
    pub icon: String,
}

/// One text the surface shows, already composed and coloured. A text wider
/// than `max_width` moves at `speed` pixels a second in `direction`; with
/// `loop_thirds` the text is three copies of one segment and wraps by a third.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub x: u32,
    pub y: u32,
    pub style: TextStyle,
    pub size: u32,
    pub color: [u8; 3],
    pub max_width: u32,
    pub text: String,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub direction: ScrollDirection,
    #[serde(default)]
    pub loop_thirds: bool,
}

/// The album art to show: the box from the skin and the file holding the picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Art {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub file: String,
    #[serde(default)]
    pub mask: String,
    #[serde(default)]
    pub border: u32,
    #[serde(default = "white")]
    pub border_color: [u8; 3],
}

fn white() -> [u8; 3] {
    [255, 255, 255]
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
    #[serde(default)]
    pub art: Option<Art>,
    #[serde(default)]
    pub type_area: Option<TypeArea>,
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
            art: None,
            type_area: None,
        }
    }
}

/// The art box with its picture, once the picture is on disk.
pub fn art(skin: &SkinDesc, meta: &Metadata) -> Option<Art> {
    let spec = skin.art.as_ref()?;
    if meta.art_file.is_empty() {
        return None;
    }
    Some(Art {
        x: spec.x,
        y: spec.y,
        w: spec.w,
        h: spec.h,
        file: meta.art_file.clone(),
        mask: spec.mask.clone(),
        border: spec.border,
        border_color: spec.border_color,
    })
}

/// The type area for the reported track type. Nothing when the type is
/// empty, so the area stays clear between tracks.
pub fn type_area(skin: &SkinDesc, meta: &Metadata) -> Option<TypeArea> {
    let spec = skin.type_area.as_ref()?;
    let key = format_key(&meta.track_type);
    if key.is_empty() {
        return None;
    }
    Some(TypeArea {
        x: spec.x,
        y: spec.y,
        box_size: spec.box_size,
        mode: spec.mode,
        align: spec.align,
        color: spec.color,
        font_size: spec.font_size,
        font_style: spec.font_style,
        label: format_label(&key),
        icon: meta.type_icon.clone(),
    })
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
        align: spec.align,
        speed: spec.speed,
        direction: ScrollDirection::Bounce,
        loop_thirds: false,
    }
}

/// The ticker line as the player composes it: artist, title and album
/// joined by the separator with its spaces, then `Next: Artist - Title`
/// when asked and known, then the end spaces; three copies for a seamless loop.
pub fn ticker_line(skin: &SkinDesc, meta: &Metadata) -> Option<Text> {
    let ticker = skin.ticker.as_ref()?;
    let space = " ".repeat(ticker.space_between as usize);
    let between = format!("{space}{}{space}", ticker.separator);
    let parts: Vec<&str> = [meta.artist.as_str(), meta.title.as_str(), meta.album.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    let mut content = parts.join(&between);
    if ticker.append_next {
        let next: Vec<&str> = [meta.next_artist.as_str(), meta.next_title.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect();
        if !next.is_empty() {
            let next_part = format!("Next: {}", next.join(" - "));
            content = if content.is_empty() {
                next_part
            } else {
                format!("{content}{between}{next_part}")
            };
        }
    }
    if content.is_empty() {
        return None;
    }
    let segment = format!("{content}{}", " ".repeat(ticker.end_spaces as usize));
    let mut line = text(&ticker.text, segment.repeat(3), ticker.text.color);
    line.direction = ticker.direction;
    line.loop_thirds = true;
    Some(line)
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
    let ticker_replaces = skin.ticker.as_ref().is_some_and(|t| t.replace);
    if !ticker_replaces {
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
        for (spec, value) in [
            (&skin.next_title, &meta.next_title),
            (&skin.next_artist, &meta.next_artist),
            (&skin.next_album, &meta.next_album),
        ] {
            if let (Some(spec), false) = (spec, value.is_empty()) {
                out.push(text(spec, value.clone(), spec.color));
            }
        }
    }
    if let Some(spec) = &skin.sample {
        // Samplerate and bit depth when the player sends them, else the
        // bitrate, else nothing.
        let line = format!("{} {}", meta.samplerate, meta.bitdepth);
        let line = line.trim();
        let line = if line.is_empty() { meta.bitrate.trim() } else { line };
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
    if let Some(line) = ticker_line(skin, meta) {
        out.push(line);
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
        art: art(skin, &input.metadata),
        type_area: type_area(skin, &input.metadata),
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
            align: TextAlign::Left,
            speed: 40.0,
        }
    }

    #[test]
    fn the_ticker_loops_one_line_and_can_replace_the_others() {
        let mut skin = SkinDesc::basic();
        skin.title = Some(spec(TextStyle::Bold, [1, 1, 1]));
        skin.next_title = Some(spec(TextStyle::Regular, [2, 2, 2]));
        skin.ticker = Some(lead::TickerSpec {
            text: TextSpec { max_width: 720, ..spec(TextStyle::Regular, [9, 9, 9]) },
            direction: ScrollDirection::Ltr,
            separator: "-".into(),
            space_between: 1,
            end_spaces: 2,
            append_next: true,
            replace: false,
        });
        let meta = Metadata {
            title: "Wonder".into(),
            artist: "Courtney".into(),
            album: String::new(),
            next_title: "Next Song".into(),
            next_artist: "Someone".into(),
            ..Metadata::default()
        };
        let lines = texts(&skin, &meta);
        let ticker = lines.last().unwrap();
        let segment = "Courtney - Wonder - Next: Someone - Next Song  ";
        assert_eq!(ticker.text, segment.repeat(3));
        assert!(ticker.loop_thirds && ticker.direction == ScrollDirection::Ltr);
        assert_eq!(lines.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()[..2], ["Wonder", "Next Song"]);

        skin.ticker.as_mut().unwrap().replace = true;
        let lines = texts(&skin, &meta);
        assert_eq!(lines.len(), 1, "replace hides the separate lines");
        assert_eq!(ticker_line(&skin, &Metadata::default()), None, "nothing to say, no ticker");
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
            ..Metadata::default()
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

    #[test]
    fn art_shows_once_its_file_exists() {
        let mut skin = SkinDesc::basic();
        let waiting = Metadata { albumart: "https://x/c.jpg".into(), ..Metadata::default() };
        assert_eq!(art(&skin, &waiting), None);
        skin.art = Some(lead::ArtSpec {
            x: 36,
            y: 25,
            w: 201,
            h: 201,
            mask: "/t/mask.png".into(),
            border: 2,
            border_color: [1, 2, 3],
        });
        assert_eq!(art(&skin, &waiting), None);
        let ready = Metadata { art_file: "/tmp/glass-art/1.img".into(), ..waiting };
        let placed = art(&skin, &ready).unwrap();
        assert_eq!((placed.x, placed.y, placed.w, placed.h), (36, 25, 201, 201));
        assert_eq!(placed.file, "/tmp/glass-art/1.img");
        assert_eq!((placed.mask.as_str(), placed.border, placed.border_color), ("/t/mask.png", 2, [1, 2, 3]));
    }

    #[test]
    fn type_area_carries_label_and_icon_and_sample_falls_back_to_bitrate() {
        let mut skin = SkinDesc::basic();
        skin.type_area = Some(lead::TypeSpec {
            x: 847,
            y: 149,
            box_size: Some((45, 45)),
            mode: TypeMode::Icon,
            align: TypeAlign::Center,
            color: [204, 176, 97],
            font_size: 20,
            font_style: TextStyle::Regular,
        });
        skin.sample = Some(spec(TextStyle::Regular, [9, 9, 9]));
        let meta = Metadata {
            track_type: "WebRadio".into(),
            type_icon: "/icons/radio.svg".into(),
            bitrate: "192 Kbps".into(),
            ..Metadata::default()
        };
        let area = type_area(&skin, &meta).unwrap();
        assert_eq!((area.label.as_str(), area.icon.as_str()), ("Webradio", "/icons/radio.svg"));
        assert_eq!((area.mode, area.align, area.font_size), (TypeMode::Icon, TypeAlign::Center, 20));
        assert_eq!(texts(&skin, &meta)[0].text, "192 Kbps");
        assert_eq!(type_area(&skin, &Metadata::default()), None);
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
