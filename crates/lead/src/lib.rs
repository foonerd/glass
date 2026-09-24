//! FIFO bytes become these types. This station does not open devices or draw.

/// Installed meter pipe. The ALSA scope writes it.
pub const METER_FIFO: &str = "/tmp/myfifo";

/// Installed spectrum pipe. The ALSA scope writes it.
pub const SPECTRUM_FIFO: &str = "/tmp/myfifosa";

/// Bin count in the Volumio ALSA template.
pub const DEFAULT_SPECTRUM_BINS: usize = 20;

/// UI full-scale that matches `meter_max`.
pub const DEFAULT_METER_MAX: f32 = 100.0;

/// `spectrum_max` written by the scope.
pub const DEFAULT_SPECTRUM_MAX: f32 = 100.0;

/// `[current] frame.rate` when the file or the key is missing.
pub const DEFAULT_FRAME_RATE: u32 = 30;

/// Inclusive range of the Volumio frame-rate control.
pub const MIN_FRAME_RATE: u32 = 10;
pub const MAX_FRAME_RATE: u32 = 60;

/// Plugin `config.txt`. The UI writes `frame.rate` into `[current]`.
pub const CONFIG_TXT: &str =
    "/data/plugins/user_interface/peppy_screensaver/screensaver/peppymeter/config.txt";

/// Left and right in UI units, plus mono derived from them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    pub left: f32,
    pub right: f32,
    pub mono: f32,
}

/// Latest spectrum frame, raw scope units. Older frames are discarded upstream.
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

/// Geometry the scene is plotted into. Pixels stay in `expose`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinDesc {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub meter_max: f32,
    pub spectrum_max: f32,
    /// Directory of the selected theme, empty when no config is loaded.
    pub theme_dir: String,
    /// Background file name inside `theme_dir`, from the theme's `meters.txt`.
    pub background: String,
    /// `left.x` and `left.y` from the selected meter. Absent on a theme-less frame.
    pub left_at: Option<(u32, u32)>,
    /// `right.x` and `right.y` from the selected meter.
    pub right_at: Option<(u32, u32)>,
    /// `indicator.filename` from the selected meter.
    pub indicator: String,
    /// Circular needle sweep in degrees, start then stop.
    pub needle: Option<(f32, f32)>,
    pub title_at: Option<(u32, u32)>,
    pub artist_at: Option<(u32, u32)>,
}

impl Default for SkinDesc {
    fn default() -> Self {
        Self::basic()
    }
}

impl SkinDesc {
    pub fn basic() -> Self {
        Self {
            name: "basic".into(),
            width: 800,
            height: 480,
            meter_max: DEFAULT_METER_MAX,
            spectrum_max: DEFAULT_SPECTRUM_MAX,
            theme_dir: String::new(),
            background: String::new(),
            left_at: None,
            right_at: None,
            indicator: String::new(),
            needle: None,
            title_at: None,
            artist_at: None,
        }
    }
}

/// Last complete meter record: little-endian `u16` left, then right.
///
/// The scope packs the same bits as `left + (right << 16)`.
pub fn decode_meter(record: &[u8]) -> Option<(u16, u16)> {
    if record.len() != 4 {
        return None;
    }
    let left = u16::from_le_bytes([record[0], record[1]]);
    let right = u16::from_le_bytes([record[2], record[3]]);
    Some((left, right))
}

/// One spectrum frame: `bin_count` little-endian `i32` values.
pub fn decode_spectrum(record: &[u8], bin_count: usize) -> Option<Vec<f32>> {
    if bin_count == 0 || record.len() != bin_count * 4 {
        return None;
    }
    let mut values = Vec::with_capacity(bin_count);
    for chunk in record.chunks_exact(4) {
        let raw = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        values.push(raw.max(0) as f32);
    }
    Some(values)
}

/// Map a scope reading onto the UI scale. Values above the pipe max clamp.
pub fn scale_level(raw: u16, max_pipe: f32, max_ui: f32) -> f32 {
    if max_pipe <= 0.0 || max_ui <= 0.0 {
        return 0.0;
    }
    (raw as f32 / max_pipe * max_ui).clamp(0.0, max_ui)
}

/// Mono is the average of the two channels.
pub fn mono_average(left: f32, right: f32) -> f32 {
    (left + right) / 2.0
}

/// Read `frame.rate` from `[current]`. Missing or invalid text is 30.
/// Values are clamped to the UI range, 10 through 60.
pub fn frame_rate_from_config(text: &str) -> u32 {
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "frame.rate" {
            continue;
        }
        return value
            .trim()
            .parse::<u32>()
            .unwrap_or(DEFAULT_FRAME_RATE)
            .clamp(MIN_FRAME_RATE, MAX_FRAME_RATE);
    }
    DEFAULT_FRAME_RATE
}

/// Screen size from `[current]`. `meter.folder` names it as `480x320` or
/// `480x320-text`. `screen.width` and `screen.height` override that pair
/// when both are set. Missing text is 800×480.
pub fn screen_from_config(text: &str) -> (u32, u32) {
    let mut folder = String::new();
    let mut width = None;
    let mut height = None;
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "meter.folder" => folder = value.trim().to_string(),
            "screen.width" => width = value.trim().parse::<u32>().ok().filter(|n| *n > 0),
            "screen.height" => height = value.trim().parse::<u32>().ok().filter(|n| *n > 0),
            _ => {}
        }
    }
    let (folder_w, folder_h) = size_from_folder(&folder).unwrap_or((800, 480));
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        _ => (folder_w, folder_h),
    }
}

fn size_from_folder(name: &str) -> Option<(u32, u32)> {
    let (width, rest) = name.split_once('x')?;
    let width: u32 = width.parse().ok()?;
    let height: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let height: u32 = height.parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

/// Value of one key in `[current]`.
pub fn current_value(text: &str, wanted: &str) -> Option<String> {
    let mut in_current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_current = line.eq_ignore_ascii_case("[current]");
            continue;
        }
        if !in_current || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == wanted {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Background file for the meter named in config. `random` and `list` use the
/// first meter in the file. `screen.bgr` wins when that meter sets it.
pub fn meter_background(meters_txt: &str, meter: &str) -> Option<String> {
    let mut sections: Vec<(String, String, String)> = Vec::new();
    let mut name = String::new();
    let mut bgr = String::new();
    let mut screen = String::new();
    let mut in_section = false;
    let flush = |sections: &mut Vec<(String, String, String)>, name: &mut String, bgr: &mut String, screen: &mut String, in_section: &mut bool| {
        if *in_section && !name.is_empty() {
            sections.push((std::mem::take(name), std::mem::take(bgr), std::mem::take(screen)));
        }
        *in_section = false;
    };
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(&mut sections, &mut name, &mut bgr, &mut screen, &mut in_section);
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "bgr.filename" => bgr = value.trim().to_string(),
            "screen.bgr" => screen = value.trim().to_string(),
            _ => {}
        }
    }
    flush(&mut sections, &mut name, &mut bgr, &mut screen, &mut in_section);
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _, _)| n == meter)
    } else {
        None
    };
    let (_, bgr, screen) = section.or_else(|| sections.first())?;
    if !screen.is_empty() {
        return Some(screen.clone());
    }
    if !bgr.is_empty() {
        return Some(bgr.clone());
    }
    None
}

/// Channel origins from the selected meter. `random` and `list` use the first meter.
pub fn meter_at(meters_txt: &str, meter: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let mut sections: Vec<(String, Option<(u32, u32)>, Option<(u32, u32)>)> = Vec::new();
    let mut name = String::new();
    let mut left_x = None;
    let mut left_y = None;
    let mut right_x = None;
    let mut right_y = None;
    let mut meter_x = 0u32;
    let mut meter_y = 0u32;
    let mut in_section = false;
    let flush = |sections: &mut Vec<(String, Option<(u32, u32)>, Option<(u32, u32)>)>,
                 name: &mut String,
                 left_x: &mut Option<u32>,
                 left_y: &mut Option<u32>,
                 right_x: &mut Option<u32>,
                 right_y: &mut Option<u32>,
                 meter_x: &mut u32,
                 meter_y: &mut u32,
                 in_section: &mut bool| {
        if *in_section && !name.is_empty() {
            let left = match (*left_x, *left_y) {
                (Some(x), Some(y)) => Some((x + *meter_x, y + *meter_y)),
                _ => None,
            };
            let right = match (*right_x, *right_y) {
                (Some(x), Some(y)) => Some((x + *meter_x, y + *meter_y)),
                _ => None,
            };
            sections.push((std::mem::take(name), left, right));
        }
        *left_x = None;
        *left_y = None;
        *right_x = None;
        *right_y = None;
        *meter_x = 0;
        *meter_y = 0;
        *in_section = false;
    };
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            flush(
                &mut sections,
                &mut name,
                &mut left_x,
                &mut left_y,
                &mut right_x,
                &mut right_y,
                &mut meter_x,
                &mut meter_y,
                &mut in_section,
            );
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let parsed = value.trim().parse::<u32>().ok();
        match key.trim() {
            "left.x" | "left.origin.x" => left_x = parsed,
            "left.y" | "left.origin.y" => left_y = parsed,
            "right.x" | "right.origin.x" => right_x = parsed,
            "right.y" | "right.origin.y" => right_y = parsed,
            "meter.x" => meter_x = parsed.unwrap_or(0),
            "meter.y" => meter_y = parsed.unwrap_or(0),
            _ => {}
        }
    }
    flush(
        &mut sections,
        &mut name,
        &mut left_x,
        &mut left_y,
        &mut right_x,
        &mut right_y,
        &mut meter_x,
        &mut meter_y,
        &mut in_section,
    );
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _, _)| n == meter)
    } else {
        None
    };
    section
        .or_else(|| sections.first())
        .map(|(_, left, right)| (*left, *right))
        .unwrap_or((None, None))
}

/// `indicator.filename` for the selected meter. `random` and `list` use the first meter.
pub fn meter_indicator(meters_txt: &str, meter: &str) -> Option<String> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut name = String::new();
    let mut indicator = String::new();
    let mut in_section = false;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if in_section && !name.is_empty() {
                sections.push((std::mem::take(&mut name), std::mem::take(&mut indicator)));
            }
            name = title.trim().to_string();
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "indicator.filename" {
            indicator = value.trim().to_string();
        }
    }
    if in_section && !name.is_empty() {
        sections.push((name, indicator));
    }
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let section = if named {
        sections.iter().find(|(n, _)| n == meter)
    } else {
        None
    };
    section
        .or_else(|| sections.first())
        .map(|(_, file)| file.clone())
        .filter(|file| !file.is_empty())
}

/// `start.angle` and `stop.angle` for the selected meter.
pub fn meter_needle(meters_txt: &str, meter: &str) -> Option<(f32, f32)> {
    let mut current = String::new();
    let mut found_start = None;
    let mut found_stop = None;
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take && found_start.is_some() && found_stop.is_some() {
                return Some((found_start.unwrap(), found_stop.unwrap()));
            }
            current = title.trim().to_string();
            take = !named || current == meter;
            if take {
                found_start = None;
                found_stop = None;
            }
            continue;
        }
        if !take {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let parsed = value.trim().parse::<f32>().ok();
        match key.trim() {
            "start.angle" => found_start = parsed,
            "stop.angle" => found_stop = parsed,
            _ => {}
        }
    }
    match (found_start, found_stop) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

fn pair_pos(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split(',');
    let x = parts.next()?.trim().parse().ok()?;
    let y = parts.next()?.trim().parse().ok()?;
    Some((x, y))
}

/// `playinfo.title.pos` and `playinfo.artist.pos` as x,y. The style word is ignored.
pub fn meter_text_at(meters_txt: &str, meter: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let mut title = None;
    let mut artist = None;
    let named = meter != "random" && meter != "list" && !meter.is_empty();
    let mut take = !named;
    for line in meters_txt.lines() {
        let line = line.trim();
        if let Some(title_name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if take && (title.is_some() || artist.is_some()) && named {
                break;
            }
            take = !named || title_name.trim() == meter;
            if take && named {
                title = None;
                artist = None;
            }
            continue;
        }
        if !take {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "playinfo.title.pos" => title = pair_pos(value),
            "playinfo.artist.pos" => artist = pair_pos(value),
            _ => {}
        }
    }
    (title, artist)
}

/// Sleep between steps for a frame rate in frames per second.
pub fn frame_period(rate: u32) -> std::time::Duration {
    let rate = rate.clamp(MIN_FRAME_RATE, MAX_FRAME_RATE);
    std::time::Duration::from_nanos(1_000_000_000 / u64::from(rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_record_is_left_then_right() {
        let (left, right) = decode_meter(&[50, 0, 25, 0]).unwrap();
        assert_eq!((left, right), (50, 25));
    }

    #[test]
    fn spectrum_record_is_little_endian_bins() {
        let bins = decode_spectrum(&[100, 0, 0, 0, 0, 0, 0, 0], 2).unwrap();
        assert_eq!(bins, vec![100.0, 0.0]);
    }

    #[test]
    fn scale_clamps_at_full() {
        assert_eq!(scale_level(150, 100.0, 100.0), 100.0);
        assert_eq!(scale_level(50, 100.0, 100.0), 50.0);
    }

    #[test]
    fn frame_rate_comes_from_current_and_clamps() {
        let text = "[sdl.env]\nframe.rate = 10\n\n[current]\nframe.rate = 15\n";
        assert_eq!(frame_rate_from_config(text), 15);
        assert_eq!(frame_rate_from_config("[current]\nframe.rate = 99\n"), 60);
        assert_eq!(frame_rate_from_config(""), DEFAULT_FRAME_RATE);
    }

    #[test]
    fn screen_size_follows_the_folder_then_the_override() {
        let folder = "[current]\nmeter.folder = 480x320-wide\nscreen.width =\nscreen.height =\n";
        assert_eq!(screen_from_config(folder), (480, 320));
        let override_size = "[current]\nmeter.folder = 480x320\nscreen.width = 1920\nscreen.height = 1080\n";
        assert_eq!(screen_from_config(override_size), (1920, 1080));
        assert_eq!(screen_from_config(""), (800, 480));
    }

    #[test]
    fn a_named_meter_uses_its_background() {
        let text = "[bar]\nbgr.filename = bar-bgr.png\nscreen.bgr =\n\n[blue]\nbgr.filename = blue-bgr.png\nscreen.bgr = blue-screen.png\n";
        assert_eq!(meter_background(text, "bar").as_deref(), Some("bar-bgr.png"));
        assert_eq!(meter_background(text, "blue").as_deref(), Some("blue-screen.png"));
        assert_eq!(meter_background(text, "random").as_deref(), Some("bar-bgr.png"));
    }

    #[test]
    fn meter_positions_come_from_the_named_section() {
        let text = "[bar]\nleft.x = 130\nleft.y = 113\nright.x = 130\nright.y = 178\n";
        assert_eq!(meter_at(text, "bar"), (Some((130, 113)), Some((130, 178))));
        let needle = "[gold]\nleft.origin.x = 333\nleft.origin.y = 305\nmeter.x = 0\nmeter.y = 124\n";
        assert_eq!(meter_at(needle, "gold").0, Some((333, 429)));
        assert_eq!(
            meter_indicator("[bar]\nindicator.filename = bar-indicator.png\n", "bar").as_deref(),
            Some("bar-indicator.png")
        );
    }
}
