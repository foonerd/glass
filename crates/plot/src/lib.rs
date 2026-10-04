//! Turn an [`lead::Input`] and a skin into a [`Scene`].
//! Pure: no files, no devices, no pixels.

use lead::{
    format_key, format_label, FanartSpec, FolderLayerSpec, IndicatorsSpec, Input, Look, Metadata,
    MeterSpec, ReelsSpec, ScrollDirection, SkinDesc, SpectrumSpec, StateLook, TextAlign, TextSpec,
    TextStyle, TonearmSpec, TypeAlign, TypeMode, VinylSpec,
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
    /// A font file of its own, or empty for the style's font.
    #[serde(default)]
    pub font_file: String,
    /// With no box of its own (`max_width` zero): a text whose width, set
    /// in the same type, is the box this one is aligned in. Empty for none.
    #[serde(default)]
    pub box_as: String,
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
    /// The art turns: cut to a circle without a mask, with a spindle and ring.
    #[serde(default)]
    pub rotation: bool,
    #[serde(default)]
    pub rpm: f32,
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
    pub left_at: Option<(i32, i32)>,
    pub right_at: Option<(i32, i32)>,
    pub needle: Option<(f32, f32, f32)>,
    #[serde(default)]
    pub texts: Vec<Text>,
    #[serde(default)]
    pub art: Option<Art>,
    #[serde(default)]
    pub type_area: Option<TypeArea>,
    /// The mono level, a fraction from 0 to 1, for one-channel meters.
    #[serde(default)]
    pub mono: f32,
    #[serde(default)]
    pub meter: MeterSpec,
    /// The spectrum boxes the meter shows, in order, and for each of the
    /// previous engine's kind the bar height in pixels per bin.
    #[serde(default)]
    pub spectra: Vec<SpectrumSpec>,
    #[serde(default)]
    pub bar_heights: Vec<Vec<u32>>,
    /// The analysers of the spectrum sections with a `style`, one per box
    /// in the same order: the look, the box and the bands' levels per
    /// channel, ready to draw.
    #[serde(default)]
    pub analysers: Vec<Analyser>,
    /// The skin's folder layers, each with the file found for this track, or empty.
    #[serde(default)]
    pub folder_layers: Vec<FolderLayer>,
    /// The fanart slot with the picture on show and its transition, when the skin has one.
    #[serde(default)]
    pub fanart: Option<Fanart>,
    /// How turning pictures are paced, as `rotation.quality` sets it: how
    /// many times a second a record, a reel or turning art is drawn anew,
    /// and in steps of how many degrees. None draws every frame at the
    /// exact angle.
    #[serde(default)]
    pub turn_pace: Option<(u32, u32)>,
    /// Whether the player plays, and whether a stop or pause is only a transition.
    #[serde(default)]
    pub playing: bool,
    #[serde(default)]
    pub transitional: bool,
    /// How far through the track, 0 to 100, and the seconds left; `None` without a length.
    #[serde(default)]
    pub progress_pct: f32,
    #[serde(default)]
    pub time_remaining: Option<f32>,
    /// The record under the art and its picture for this track.
    #[serde(default)]
    pub vinyl: Option<Vinyl>,
    #[serde(default)]
    pub tonearm: Option<TonearmSpec>,
    /// The cassette reels and the pictures they show for this track.
    #[serde(default)]
    pub reels: Option<Reels>,
    /// The indicators with their states for this frame.
    #[serde(default)]
    pub indicators: Option<Indicators>,
}

/// The indicators of the scene: the volume 0 to 100, the state index of
/// mute (off, on, zero), shuffle (off, on, legacy infinity), repeat (off,
/// all, single, infinity) and play state (stop, pause, play), and the
/// progress 0 to 100.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Indicators {
    pub spec: IndicatorsSpec,
    pub volume: u32,
    pub mute_state: usize,
    pub shuffle_state: usize,
    pub repeat_state: usize,
    pub play_state: usize,
    pub progress: u32,
    /// One per button of the spec: whether it shows its active picture,
    /// by its action and the player's state; a finger on it counts too.
    #[serde(default)]
    pub buttons_active: Vec<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reels {
    pub spec: ReelsSpec,
    pub left_file: String,
    pub right_file: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vinyl {
    pub spec: VinylSpec,
    pub file: String,
}

/// The fanart slot of the scene: its box, the picture on show, the one it
/// replaces while a transition runs, and where the transition stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fanart {
    pub spec: FanartSpec,
    pub file: String,
    pub prev_file: String,
    pub transition: String,
    pub transition_ms: u32,
    pub elapsed_ms: u32,
}

/// One folder layer of the scene: its box and the picture file to show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderLayer {
    pub spec: FolderLayerSpec,
    pub file: String,
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
            mono: 0.0,
            meter: MeterSpec::default(),
            spectra: Vec::new(),
            bar_heights: Vec::new(),
            analysers: Vec::new(),
            folder_layers: Vec::new(),
            fanart: None,
            turn_pace: None,
            playing: false,
            transitional: false,
            progress_pct: 0.0,
            time_remaining: None,
            vinyl: None,
            tonearm: None,
            reels: None,
            indicators: None,
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
        rotation: spec.rotation,
        rpm: spec.rpm * skin.rotation.speed,
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
        label: match spec.label {
            lead::TypeLabel::SampleRate => {
                let line = sample_line(meta);
                if line.is_empty() {
                    format_label(&key)
                } else {
                    line
                }
            }
            lead::TypeLabel::Format => format_label(&key),
        },
        icon: meta.type_icon.clone(),
    })
}

/// An analyser ready to draw: the section's look, the box on screen, and
/// for each band drawn its edges in hertz, its level and its peak hold per
/// channel on the bar scale (0 an empty bar, 1 a full one).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Analyser {
    pub look: Look,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub edges: Vec<(f32, f32)>,
    pub levels: [Vec<f32>; 2],
    pub hold: [Vec<f32>; 2],
    /// Whether two channels are drawn: the look lays out two and the
    /// section did not ask for a one-channel bank. What the channels read
    /// never decides it: a recording alike on both draws both.
    pub stereo: bool,
    pub onsets: u8,
}

/// A band's amplitude (a full-scale sine reading 1.0) on the bar scale
/// the look asks for: by decibels between the level range's ends, or by
/// amplitude raised to one over the boost.
pub fn bar_level(look: &Look, amplitude: f32, weighting_db: f32) -> f32 {
    let amplitude = amplitude.max(0.0) * 10f32.powf(weighting_db / 20.0);
    if look.level_linear {
        amplitude
            .clamp(0.0, 1.0)
            .powf(1.0 / look.level_boost.max(1.0))
    } else {
        if amplitude <= 0.0 {
            return 0.0;
        }
        let db = 20.0 * amplitude.log10();
        let (lo, hi) = look.level_range;
        ((db - lo) / (hi - lo).max(1.0)).clamp(0.0, 1.0)
    }
}

/// The analyser of a spectrum section with a look, from the bank in the
/// input: the bands inside the look's range, each weighted by its centre
/// and put on the bar scale.
fn analyser(spec: &SpectrumSpec, input: &Input) -> Option<Analyser> {
    let look = spec.look.clone()?;
    let bins = input.bins.bank[0].len();
    if bins == 0 {
        return Some(Analyser {
            look,
            x: spec.x,
            y: spec.y,
            w: spec.w,
            h: spec.h,
            ..Analyser::default()
        });
    }
    let edges_all = bank::edges(bins, input.bins.scale);
    let (lo, hi) = look.range;
    let drawn: Vec<usize> = (0..bins)
        .filter(|&i| {
            let (a, b) = edges_all[i];
            b > lo && a < hi
        })
        .collect();
    let edges: Vec<(f32, f32)> = drawn.iter().map(|&i| edges_all[i]).collect();
    let weights: Vec<f32> = edges
        .iter()
        .map(|&(a, b)| look.weighting.gain_db((a * b).sqrt()))
        .collect();
    let map = |bands: &[f32]| -> Vec<f32> {
        drawn
            .iter()
            .zip(weights.iter())
            .map(|(&i, &w)| bar_level(&look, bands.get(i).copied().unwrap_or(0.0), w))
            .collect()
    };
    // How many channels are drawn is the theme's to say, never the
    // sound's: a section that asks for a one-channel bank (`channels = 1`)
    // draws one, any other draws what its layout asks. Two channels that
    // read alike (a mono recording, silence) are still two: a look laid
    // out for two keeps its two sides and their palettes through them.
    let one = spec.demand.is_some_and(|d| d.channels < 2);
    // One channel drawn from the bank: the mean of the two, band by band,
    // before the bar scale, which for a one-channel bank is that channel;
    // or, when the box names a channel, that channel alone.
    let mean = |a: &[f32], b: &[f32]| -> Vec<f32> {
        if a.len() != b.len() {
            return a.to_vec();
        }
        a.iter().zip(b.iter()).map(|(l, r)| (l + r) / 2.0).collect()
    };
    let (levels, hold, stereo) = if spec.channel != lead::SpectrumChannel::Mean {
        let ch = usize::from(spec.channel == lead::SpectrumChannel::Right);
        (
            [map(&input.bins.bank[ch]), Vec::new()],
            [map(&input.bins.hold[ch]), Vec::new()],
            false,
        )
    } else if one || matches!(look.layout, lead::Layout::Single) {
        (
            [
                map(&mean(&input.bins.bank[0], &input.bins.bank[1])),
                Vec::new(),
            ],
            [
                map(&mean(&input.bins.hold[0], &input.bins.hold[1])),
                Vec::new(),
            ],
            false,
        )
    } else {
        (
            [map(&input.bins.bank[0]), map(&input.bins.bank[1])],
            [map(&input.bins.hold[0]), map(&input.bins.hold[1])],
            true,
        )
    };
    Some(Analyser {
        look,
        x: spec.x,
        y: spec.y,
        w: spec.w,
        h: spec.h,
        edges,
        levels,
        hold,
        stereo,
        onsets: input.bins.onsets,
    })
}

/// The sample rate line: samplerate and bit depth when the player sends
/// them, else the bitrate, else nothing.
pub fn sample_line(meta: &Metadata) -> String {
    let line = format!("{} {}", meta.samplerate, meta.bitdepth);
    let line = line.trim();
    if line.is_empty() {
        meta.bitrate.trim().to_string()
    } else {
        line.to_string()
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
        align: spec.align,
        speed: spec.speed,
        direction: ScrollDirection::Bounce,
        loop_thirds: false,
        font_file: spec.font_file.clone(),
        box_as: spec.box_as.clone(),
    }
}

/// Persist countdown, as the player colours it.
const PERSIST_COLOR: [u8; 3] = [242, 165, 0];

fn clock(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// The ticker line as the player composes it: artist, title and album
/// joined by the separator with its spaces, then `Next: Artist - Title`
/// when asked and known, then the end spaces; three copies for a seamless loop.
pub fn ticker_line(skin: &SkinDesc, meta: &Metadata) -> Option<Text> {
    let ticker = skin.ticker.as_ref()?;
    let space = " ".repeat(ticker.space_between as usize);
    let between = format!("{space}{}{space}", ticker.separator);
    let parts: Vec<&str> = [
        meta.artist.as_str(),
        meta.title.as_str(),
        meta.album.as_str(),
    ]
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
    // The player counts whole seconds of position first, then subtracts, so
    // 218.051 with 4.356 played shows 214, not 213.
    let played = meta.seek.max(0.0).floor();
    Some((meta.duration - played).max(0.0).floor() as u32)
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
        let line = sample_line(meta);
        if !line.is_empty() {
            out.push(text(spec, line, spec.color));
        }
    }
    if let Some(spec) = &skin.volume_value {
        let words = volume_words(
            &skin.volume_format,
            &skin.volume_mute,
            meta.volume,
            meta.mute,
        );
        out.push(text(spec, words, spec.color));
    }
    if let Some(spec) = &skin.time {
        // While the display persists after a pause in countdown mode, the
        // remaining field counts the persist period down in orange.
        if meta.status != "play" && meta.persist_mode == "countdown" {
            out.push(text(spec, clock(meta.persist_left), PERSIST_COLOR));
        } else if let Some(left) = seconds_remaining(meta) {
            let color = if (1..=10).contains(&left) {
                LAST_SECONDS
            } else {
                spec.color
            };
            out.push(text(spec, clock(left), color));
        }
    }
    if let Some(spec) = &skin.time_elapsed {
        out.push(text(spec, clock(meta.seek.max(0.0) as u32), spec.color));
    }
    if let Some(spec) = &skin.time_total {
        out.push(text(spec, clock(meta.duration.max(0.0) as u32), spec.color));
    }
    if let Some(line) = ticker_line(skin, meta) {
        out.push(line);
    }
    out
}

/// One step of the line. Headless mode may publish this and skip raster.
/// The volume as the theme words it: the number alone; inside the theme's
/// pattern where it has one (`VOL {} %`, the first `{}` taking the number; a
/// pattern with no `{}` is none); and, while the player is muted, the
/// theme's word for that where it has one.
fn volume_words(format: &str, mute: &str, volume: impl std::fmt::Display, muted: bool) -> String {
    if muted && !mute.is_empty() {
        return mute.to_string();
    }
    let number = volume.to_string();
    if format.contains("{}") {
        format.replacen("{}", &number, 1)
    } else {
        number
    }
}

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
        mono: (input.levels.mono / meter_max).clamp(0.0, 1.0),
        meter: skin.meter.clone(),
        bar_heights: skin
            .spectra
            .iter()
            .map(|spec| {
                input
                    .bins
                    .values
                    .iter()
                    .map(|&raw| spec.bar_height(raw))
                    .collect()
            })
            .collect(),
        spectra: skin.spectra.clone(),
        analysers: skin
            .spectra
            .iter()
            .filter_map(|spec| analyser(spec, input))
            .collect(),
        folder_layers: skin
            .folder_layers
            .iter()
            .enumerate()
            .map(|(i, spec)| FolderLayer {
                spec: spec.clone(),
                file: input
                    .metadata
                    .folder_files
                    .get(i)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
        fanart: skin.fanart.as_ref().map(|spec| Fanart {
            spec: spec.clone(),
            file: input.metadata.fanart_file.clone(),
            prev_file: input.metadata.fanart_prev_file.clone(),
            transition: input.metadata.fanart_transition.clone(),
            transition_ms: input.metadata.fanart_transition_ms,
            elapsed_ms: input.metadata.fanart_elapsed_ms,
        }),
        turn_pace: Some((skin.rotation.fps, skin.rotation.step)),
        playing: input.metadata.status == "play",
        transitional: input.metadata.volatile != Some(false),
        progress_pct: progress(&input.metadata).0,
        time_remaining: progress(&input.metadata).1,
        vinyl: skin.vinyl.as_ref().map(|spec| Vinyl {
            spec: spec.clone(),
            file: if input.metadata.vinyl_file.is_empty() {
                spec.theme_file.clone()
            } else {
                input.metadata.vinyl_file.clone()
            },
        }),
        tonearm: skin.tonearm.clone(),
        reels: skin.reels.as_ref().map(|spec| Reels {
            spec: spec.clone(),
            left_file: match (&spec.left, input.metadata.reel_files.0.is_empty()) {
                (Some(reel), true) => reel.theme_file.clone(),
                (Some(_), false) => input.metadata.reel_files.0.clone(),
                (None, _) => String::new(),
            },
            right_file: match (&spec.right, input.metadata.reel_files.1.is_empty()) {
                (Some(reel), true) => reel.theme_file.clone(),
                (Some(_), false) => input.metadata.reel_files.1.clone(),
                (None, _) => String::new(),
            },
        }),
        indicators: skin
            .indicators
            .as_ref()
            .map(|spec| indicators(spec, &input.metadata, progress(&input.metadata).0)),
    }
}

/// The indicator states the player's handler derives. Infinity, which the
/// plugin's channel reports, lands on the repeat indicator when the skin
/// gives it a fourth state, else on the shuffle indicator's legacy third.
fn indicators(spec: &IndicatorsSpec, meta: &Metadata, progress_pct: f32) -> Indicators {
    let repeat_has_infinity = spec.repeat.as_ref().is_some_and(|r| match &r.look {
        StateLook::Icons { files } => files.len() >= 4 && !files[3].is_empty(),
        StateLook::Led { colors, .. } => colors.len() >= 4,
    });
    let infinity = meta.infinity;
    Indicators {
        spec: spec.clone(),
        volume: meta.volume.min(100),
        mute_state: if meta.mute {
            1
        } else if meta.volume == 0 {
            2
        } else {
            0
        },
        shuffle_state: if infinity && !repeat_has_infinity {
            2
        } else if meta.random {
            1
        } else {
            0
        },
        repeat_state: if repeat_has_infinity && infinity {
            3
        } else if meta.repeat_single {
            2
        } else if meta.repeat {
            1
        } else {
            0
        },
        buttons_active: spec
            .buttons
            .iter()
            .map(|b| match b.action {
                lead::ButtonAction::Play | lead::ButtonAction::Toggle => meta.status == "play",
                lead::ButtonAction::Pause => meta.status == "pause",
                lead::ButtonAction::Stop => meta.status != "play" && meta.status != "pause",
                lead::ButtonAction::Mute => meta.mute,
                lead::ButtonAction::Random => meta.random,
                lead::ButtonAction::Repeat => meta.repeat || meta.repeat_single,
                _ => false,
            })
            .collect(),
        play_state: match meta.status.as_str() {
            "play" => 2,
            "pause" => 1,
            _ => 0,
        },
        progress: progress_pct.clamp(0.0, 100.0) as u32,
    }
}

/// How far the playback is, 0 to 100, and the seconds left: over the whole
/// queue when the player reports one and the source is not a stream, else
/// over the track; nothing without a length.
fn progress(meta: &Metadata) -> (f32, Option<f32>) {
    let seek = meta.seek.max(0.0);
    if meta.queue_total_s > 0.0 && meta.duration > 0.0 && meta.volatile != Some(true) {
        let played = meta.queue_before_s + seek;
        return (
            ((played / meta.queue_total_s) * 100.0).min(100.0),
            Some((meta.queue_total_s - played).max(0.0)),
        );
    }
    if meta.duration > 0.0 {
        (
            (seek / meta.duration * 100.0).clamp(0.0, 100.0),
            Some(meta.duration - seek),
        )
    } else {
        (0.0, None)
    }
}

#[cfg(test)]
mod tests {
    /// A button is active by what its action means in the player's state:
    /// play while playing, mute while muted, a momentary one never on its own.
    #[test]
    fn a_button_is_active_by_its_action_and_the_players_state() {
        let spec = lead::meter_indicators(
            "[m]\nconfig.extend = True\nbutton.play.pos = 1,1\nbutton.play.size = 4,4\nbutton.play.action = play\nbutton.mute.pos = 9,1\nbutton.mute.size = 4,4\nbutton.mute.action = mute\nbutton.next.pos = 20,1\nbutton.next.size = 4,4\nbutton.next.action = next\n",
            "m",
            "/t",
        )
        .expect("extended");
        let meta = Metadata {
            status: "play".into(),
            mute: true,
            ..Metadata::default()
        };
        let got = indicators(&spec, &meta, 0.0);
        // Buttons come sorted by name: mute, next, play.
        assert_eq!(got.buttons_active, vec![true, false, true]);
    }

    use super::*;
    use lead::{Bins, Input, Levels, SkinDesc};

    #[test]
    fn infinity_lands_on_repeat_with_a_fourth_state_else_on_shuffle() {
        use lead::{IndicatorsSpec, StateIndicator};
        let led = |states: usize| StateIndicator {
            x: 0,
            y: 0,
            look: StateLook::Led {
                w: 4,
                h: 4,
                circle: true,
                colors: vec![[0, 0, 0]; states],
            },
            glow: 0,
            glow_intensity: 0.5,
            glow_colors: Vec::new(),
        };
        let meta = Metadata {
            infinity: true,
            random: true,
            repeat: true,
            ..Metadata::default()
        };
        let four = IndicatorsSpec {
            repeat: Some(led(4)),
            shuffle: Some(led(3)),
            ..IndicatorsSpec::default()
        };
        let states = indicators(&four, &meta, 0.0);
        assert_eq!(states.repeat_state, 3);
        assert_eq!(
            states.shuffle_state, 1,
            "shuffle shows random when repeat owns infinity"
        );
        let three = IndicatorsSpec {
            repeat: Some(led(3)),
            shuffle: Some(led(3)),
            ..IndicatorsSpec::default()
        };
        let states = indicators(&three, &meta, 0.0);
        assert_eq!(states.repeat_state, 1, "repeat all");
        assert_eq!(states.shuffle_state, 2, "the legacy third shuffle state");
        let off = Metadata {
            random: true,
            ..Metadata::default()
        };
        assert_eq!(indicators(&three, &off, 0.0).shuffle_state, 1);
        assert_eq!(indicators(&four, &off, 0.0).repeat_state, 0);
    }

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
                ..Bins::default()
            },
            metadata: Metadata::default(),
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
            font_file: String::new(),
            box_as: String::new(),
        }
    }

    #[test]
    fn elapsed_total_and_the_persist_countdown_are_clocks() {
        let mut skin = SkinDesc::basic();
        skin.time = Some(spec(TextStyle::Digi, [180, 180, 180]));
        skin.time_elapsed = Some(spec(TextStyle::Regular, [1, 1, 1]));
        skin.time_total = Some(spec(TextStyle::Digi, [2, 2, 2]));
        let playing = Metadata {
            status: "play".into(),
            duration: 218.0,
            seek: 65.4,
            ..Metadata::default()
        };
        let lines: Vec<String> = texts(&skin, &playing).into_iter().map(|t| t.text).collect();
        assert_eq!(
            lines,
            ["02:33", "01:05", "03:38"],
            "remaining counts whole seconds played"
        );
        let persisting = Metadata {
            status: "pause".into(),
            persist_mode: "countdown".into(),
            persist_left: 9,
            ..playing.clone()
        };
        let first = texts(&skin, &persisting).remove(0);
        assert_eq!((first.text.as_str(), first.color), ("00:09", PERSIST_COLOR));
        let frozen = Metadata {
            persist_mode: "freeze".into(),
            ..persisting
        };
        assert_eq!(
            texts(&skin, &frozen)[0].text,
            "02:33",
            "freeze keeps the track time"
        );
    }

    #[test]
    fn the_ticker_loops_one_line_and_can_replace_the_others() {
        let mut skin = SkinDesc::basic();
        skin.title = Some(spec(TextStyle::Bold, [1, 1, 1]));
        skin.next_title = Some(spec(TextStyle::Regular, [2, 2, 2]));
        skin.ticker = Some(lead::TickerSpec {
            text: TextSpec {
                max_width: 720,
                ..spec(TextStyle::Regular, [9, 9, 9])
            },
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
        assert_eq!(
            lines.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()[..2],
            ["Wonder", "Next Song"]
        );

        skin.ticker.as_mut().unwrap().replace = true;
        let lines = texts(&skin, &meta);
        assert_eq!(lines.len(), 1, "replace hides the separate lines");
        assert_eq!(
            ticker_line(&skin, &Metadata::default()),
            None,
            "nothing to say, no ticker"
        );
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
        assert_eq!(
            lines,
            [
                "Wonder",
                "Courtney Barnett - Creature of Habit",
                "44.1 kHz 16-bit",
                "03:37"
            ]
        );

        skin.album = Some(spec(TextStyle::Light, [0, 0, 0]));
        let lines: Vec<String> = texts(&skin, &meta).into_iter().map(|t| t.text).collect();
        assert_eq!(lines[1], "Courtney Barnett");
        assert_eq!(lines[2], "Creature of Habit");

        let ending = Metadata {
            seek: 210.0,
            ..meta.clone()
        };
        let clock = texts(&skin, &ending).pop().unwrap();
        assert_eq!((clock.text.as_str(), clock.color), ("00:08", LAST_SECONDS));

        let stream = Metadata {
            duration: 0.0,
            ..meta
        };
        assert!(texts(&skin, &stream)
            .iter()
            .all(|t| t.style != TextStyle::Digi));
    }

    #[test]
    fn art_shows_once_its_file_exists() {
        let mut skin = SkinDesc::basic();
        let waiting = Metadata {
            albumart: "https://x/c.jpg".into(),
            ..Metadata::default()
        };
        assert_eq!(art(&skin, &waiting), None);
        skin.art = Some(lead::ArtSpec {
            x: 36,
            y: 25,
            w: 201,
            h: 201,
            mask: "/t/mask.png".into(),
            border: 2,
            border_color: [1, 2, 3],
            rotation: false,
            rpm: 0.0,
        });
        assert_eq!(art(&skin, &waiting), None);
        let ready = Metadata {
            art_file: "/tmp/glass-art/1.img".into(),
            ..waiting
        };
        let placed = art(&skin, &ready).unwrap();
        assert_eq!((placed.x, placed.y, placed.w, placed.h), (36, 25, 201, 201));
        assert_eq!(placed.file, "/tmp/glass-art/1.img");
        assert_eq!(
            (placed.mask.as_str(), placed.border, placed.border_color),
            ("/t/mask.png", 2, [1, 2, 3])
        );
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
            label: lead::TypeLabel::Format,
        });
        skin.sample = Some(spec(TextStyle::Regular, [9, 9, 9]));
        let meta = Metadata {
            track_type: "WebRadio".into(),
            type_icon: "/icons/radio.svg".into(),
            bitrate: "192 Kbps".into(),
            ..Metadata::default()
        };
        let area = type_area(&skin, &meta).unwrap();
        assert_eq!(
            (area.label.as_str(), area.icon.as_str()),
            ("Webradio", "/icons/radio.svg")
        );
        assert_eq!(
            (area.mode, area.align, area.font_size),
            (TypeMode::Icon, TypeAlign::Center, 20)
        );
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
            let mut recorded: Recorded = serde_json::from_str(&text)
                .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
            // A frame recorded before scenes carried the turning pace says
            // nothing of it: the pace is then the skin's, as `step` gives it.
            if recorded.scene.turn_pace.is_none() {
                recorded.scene.turn_pace =
                    Some((recorded.skin.rotation.fps, recorded.skin.rotation.step));
            }
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

#[cfg(test)]
mod analyser_tests {
    use super::*;
    use lead::{Bins, Levels, Look, SpectrumSpec};

    #[test]
    fn a_band_lands_on_the_bar_scale_by_decibels_or_by_amplitude() {
        let look = Look::default();
        assert_eq!(bar_level(&look, 1.0, 0.0), 1.0, "0 dB is over the top");
        let half = 10f32.powf(-30.0 / 20.0);
        assert!(
            (bar_level(&look, half, 0.0) - 0.5).abs() < 0.01,
            "-30 dB is half way"
        );
        assert_eq!(bar_level(&look, 0.0, 0.0), 0.0);
        assert!(
            (bar_level(&look, half, 6.0) - 0.6).abs() < 0.01,
            "6 dB of weighting lifts it"
        );
        let linear = Look {
            level_linear: true,
            level_boost: 2.0,
            ..Look::default()
        };
        assert!((bar_level(&linear, 0.25, 0.0) - 0.5).abs() < 0.001);
        assert_eq!(bar_level(&linear, 1.0, 0.0), 1.0);
    }

    #[test]
    fn the_analyser_takes_the_bands_in_its_range_per_channel() {
        let mut skin = SkinDesc::default();
        let look = Look {
            range: (100.0, 10_000.0),
            layout: lead::Layout::DualVertical,
            ..Look::default()
        };
        skin.spectra = vec![SpectrumSpec {
            x: 40,
            y: 40,
            w: 1200,
            h: 560,
            look: Some(look),
            ..SpectrumSpec::default()
        }];
        let bands = 32;
        let mut left = vec![0.0f32; bands];
        let mut right = vec![0.0f32; bands];
        left[16] = 1.0;
        // Minus 30 dB: half way up the default decibel range.
        right[16] = 10f32.powf(-30.0 / 20.0);
        let input = Input {
            levels: Levels::default(),
            bins: Bins {
                values: Vec::new(),
                bank: [left, right],
                hold: [vec![1.0; bands], vec![1.0; bands]],
                scale: bank::Scale::Log,
                onsets: 0b0010,
            },
            metadata: Metadata::default(),
        };
        let scene = step(&skin, &input);
        let a = scene.analysers.into_iter().next().expect("an analyser");
        assert_eq!((a.x, a.y, a.w, a.h), (40, 40, 1200, 560));
        assert!(a.stereo);
        assert_eq!(a.onsets, 0b0010);
        let all = bank::edges(bands, bank::Scale::Log);
        let expected = all
            .iter()
            .filter(|(lo, hi)| *hi > 100.0 && *lo < 10_000.0)
            .count();
        assert_eq!(a.edges.len(), expected);
        assert_eq!(a.levels[0].len(), expected);
        assert!(a.edges[0].0 < 100.0 && a.edges[expected - 1].1 > 10_000.0);
        let loudest = (0..expected)
            .max_by(|x, y| a.levels[0][*x].total_cmp(&a.levels[0][*y]))
            .unwrap();
        assert_eq!(a.levels[0][loudest], 1.0);
        assert!(
            (a.levels[1][loudest] - 0.5).abs() < 0.02,
            "the right channel half way: {}",
            a.levels[1][loudest]
        );
        assert!(a.hold[0].iter().all(|h| *h == 1.0));
        // Without a look there is no analyser; without a bank an empty one.
        skin.spectra[0].look = None;
        assert!(step(&skin, &input).analysers.is_empty());
    }

    /// Two boxes, one per channel: each analyser draws its own channel of
    /// the bank alone, in the box the meter gave it.
    #[test]
    fn two_boxes_draw_one_channel_each() {
        let mut skin = SkinDesc::default();
        for (i, channel) in [lead::SpectrumChannel::Left, lead::SpectrumChannel::Right]
            .into_iter()
            .enumerate()
        {
            skin.spectra.push(SpectrumSpec {
                x: 40 + i as i32 * 600,
                y: 40,
                w: 560,
                h: 400,
                channel,
                look: Some(Look::default()),
                ..SpectrumSpec::default()
            });
        }
        let bands = 8;
        let mut left = vec![0.0f32; bands];
        left[2] = 1.0;
        let mut right = vec![0.0f32; bands];
        right[5] = 1.0;
        let input = Input {
            levels: Levels::default(),
            bins: Bins {
                values: Vec::new(),
                bank: [left, right],
                hold: [vec![1.0; bands], vec![1.0; bands]],
                scale: bank::Scale::Log,
                onsets: 0,
            },
            metadata: Metadata::default(),
        };
        let scene = step(&skin, &input);
        assert_eq!(scene.analysers.len(), 2);
        let (a, b) = (&scene.analysers[0], &scene.analysers[1]);
        assert_eq!((a.x, b.x), (40, 640), "each in its own box");
        assert!(!a.stereo && !b.stereo, "one channel each");
        assert!(
            a.levels[0][2] > 0.99 && a.levels[0][5] < 0.01,
            "the left box shows the left channel"
        );
        assert!(
            b.levels[0][5] > 0.99 && b.levels[0][2] < 0.01,
            "the right box the right"
        );
        assert!(a.levels[1].is_empty() && b.levels[1].is_empty());
    }

    /// How many channels a look draws is the theme's to say, never the
    /// sound's: silence and a recording alike on both channels keep the two
    /// sides of a look laid out for two, each with its own levels, and a
    /// section that asks for a one-channel bank draws one whatever plays.
    #[test]
    fn what_plays_never_takes_a_side_from_a_look() {
        let skin_of = |demand: Option<bank::Demand>| SkinDesc {
            spectra: vec![SpectrumSpec {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
                demand,
                look: Some(Look {
                    layout: lead::Layout::DualHorizontal,
                    ..Look::default()
                }),
                ..SpectrumSpec::default()
            }],
            ..SkinDesc::default()
        };
        let bands = 8;
        let with = |skin: &SkinDesc, left: Vec<f32>, right: Vec<f32>| {
            let input = Input {
                levels: Levels::default(),
                bins: Bins {
                    values: Vec::new(),
                    bank: [left, right],
                    hold: [vec![0.0; bands], vec![0.0; bands]],
                    scale: bank::Scale::Log,
                    onsets: 0,
                },
                metadata: Metadata::default(),
            };
            step(skin, &input)
                .analysers
                .into_iter()
                .next()
                .expect("an analyser")
        };
        let mut tone = vec![0.0f32; bands];
        tone[3] = 1.0;
        let mut other = vec![0.0f32; bands];
        other[5] = 1.0;
        for demand in [None, Some(bank::Demand::new(bands, 2, bank::Scale::Log))] {
            let skin = skin_of(demand);
            let still = with(&skin, vec![0.0; bands], vec![0.0; bands]);
            assert!(still.stereo, "silence keeps the two sides");
            assert_eq!(
                (still.levels[0].len(), still.levels[1].len()),
                (bands, bands)
            );
            let mono = with(&skin, tone.clone(), tone.clone());
            assert!(mono.stereo, "the same sound on both keeps the two sides");
            assert_eq!(mono.levels[0], mono.levels[1], "and shows it on both");
            assert!(mono.levels[0][3] > 0.99);
            assert!(with(&skin, tone.clone(), other.clone()).stereo);
        }
        // A section that asks for one channel draws one, of whatever bank
        // the theme's other sections brought about: the two channels' mean.
        let skin = skin_of(Some(bank::Demand::new(bands, 1, bank::Scale::Log)));
        let single = with(&skin, tone.clone(), tone.clone());
        assert!(!single.stereo, "a one-channel bank draws as one");
        assert!(single.levels[1].is_empty() && single.levels[0][3] > 0.99);
        let mixed = with(&skin, tone, other);
        assert!(!mixed.stereo);
        let half = bar_level(&Look::default(), 0.5, 0.0);
        assert!(
            (mixed.levels[0][3] - half).abs() < 1e-6 && (mixed.levels[0][5] - half).abs() < 1e-6,
            "the mean of the two channels"
        );
    }

    /// A `single` layout asked of a stereo bank draws one channel: the
    /// mean of the two, band by band, as a one-channel bank would read.
    #[test]
    fn a_single_layout_over_a_stereo_bank_draws_the_mean() {
        let skin = SkinDesc {
            spectra: vec![SpectrumSpec {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
                look: Some(Look {
                    layout: lead::Layout::Single,
                    ..Look::default()
                }),
                ..SpectrumSpec::default()
            }],
            ..SkinDesc::default()
        };
        let bands = 8;
        let mut left = vec![0.0f32; bands];
        left[3] = 1.0;
        let right = vec![0.0f32; bands];
        let input = Input {
            levels: Levels::default(),
            bins: Bins {
                values: Vec::new(),
                bank: [left, right],
                hold: [vec![1.0; bands], vec![0.0; bands]],
                scale: bank::Scale::Log,
                onsets: 0,
            },
            metadata: Metadata::default(),
        };
        let a = step(&skin, &input)
            .analysers
            .into_iter()
            .next()
            .expect("an analyser");
        assert!(!a.stereo, "one channel drawn");
        assert!(a.levels[1].is_empty() && a.hold[1].is_empty());
        assert_eq!(a.levels[0].len(), bands);
        let want = bar_level(&Look::default(), 0.5, 0.0);
        assert!(
            (a.levels[0][3] - want).abs() < 1e-6,
            "the mean amplitude on the bar scale: {} against {want}",
            a.levels[0][3]
        );
        assert!((a.hold[0][3] - want).abs() < 1e-6, "the hold's mean too");
    }

    #[test]
    fn the_volume_is_worded_as_the_theme_asks() {
        assert_eq!(volume_words("", "", 45, false), "45");
        assert_eq!(volume_words("VOL {} %", "", 45, false), "VOL 45 %");
        assert_eq!(volume_words("{}%", "", 100, false), "100%");
        assert_eq!(
            volume_words("{} of {}", "", 7, false),
            "7 of {}",
            "the first takes the number"
        );
        assert_eq!(
            volume_words("VOL", "", 45, false),
            "45",
            "a pattern with no place for the number is none"
        );
        // Muted: the theme's word where it has one, else the number as ever.
        assert_eq!(volume_words("VOL {} %", "MUTE", 45, true), "MUTE");
        assert_eq!(volume_words("VOL {} %", "", 45, true), "VOL 45 %");
        assert_eq!(volume_words("VOL {} %", "MUTE", 45, false), "VOL 45 %");
    }
}
