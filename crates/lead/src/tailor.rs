//! The cutter: a theme's text files scaled to another screen. Every key
//! the parser reads is classified here by what its value is, a point on
//! the screen, a size, a length, a font size, a picture, or something
//! that does not scale, and the text is rewritten line by line with the
//! comments, the order and the keys it does not know kept as they are.
//! A test at the end holds the parser to the table: a key read anywhere
//! in the parser that the table does not name fails it.

/// The scale from one screen to another: one factor with the theme
/// centred and letterboxed, or two factors and no offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub from: (u32, u32),
    pub to: (u32, u32),
    pub sx: f32,
    pub sy: f32,
    pub ox: f32,
    pub oy: f32,
}

impl Plan {
    /// One factor, the larger that keeps the theme within the screen, and
    /// the theme centred: what is left over is the display's background.
    pub fn fit(from: (u32, u32), to: (u32, u32)) -> Self {
        let (fw, fh) = (from.0.max(1) as f32, from.1.max(1) as f32);
        let (tw, th) = (to.0.max(1) as f32, to.1.max(1) as f32);
        let s = (tw / fw).min(th / fh);
        Self {
            from,
            to,
            sx: s,
            sy: s,
            ox: ((tw - fw * s) / 2.0).floor(),
            oy: ((th - fh * s) / 2.0).floor(),
        }
    }

    /// Two factors, the theme stretched to the screen's shape.
    pub fn stretch(from: (u32, u32), to: (u32, u32)) -> Self {
        let (fw, fh) = (from.0.max(1) as f32, from.1.max(1) as f32);
        Self {
            from,
            to,
            sx: to.0.max(1) as f32 / fw,
            sy: to.1.max(1) as f32 / fh,
            ox: 0.0,
            oy: 0.0,
        }
    }

    /// The factor for a length with no direction: the smaller of the two.
    pub fn s(&self) -> f32 {
        self.sx.min(self.sy)
    }
}

/// What a key's value is, for the cutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `x,y` on the screen, a style word or more after: scaled and offset.
    Point,
    /// `x,y` relative to a meter, a picture or a widget: scaled, not offset.
    Offset,
    /// `w,h`: scaled.
    Size,
    /// One coordinate on the screen: scaled and offset.
    X,
    Y,
    /// One coordinate relative to the meter: scaled only.
    XRel,
    YRel,
    /// A width, a height, or a length with no direction.
    LenX,
    LenY,
    Len,
    /// Two lengths with no direction, `a,b`.
    Lens,
    /// A font size.
    Font,
    /// `screen.width` and `screen.height`: the new size.
    Screen,
    /// A picture, or a list of pictures, to resample.
    Picture,
    /// The screen's own picture: resampled, then laid at the letterbox
    /// offset on a sheet of the new screen's size, so the copy's backdrop
    /// fills its screen as the original's did.
    Backdrop,
    /// A share below 1, pixels from 1 up.
    Share,
    /// Angles, counts, shares, colours, names, flags: as it is.
    Keep,
}

/// A key pattern and its kind. A `*` segment matches one segment of a
/// key, a `**` segment one or more; the first match wins.
const RULES: &[(&str, Kind)] = &[
    // The screen.
    ("screen.width", Kind::Screen),
    ("screen.height", Kind::Screen),
    ("screen.bgr", Kind::Backdrop),
    // The meter: its place on screen, and everything inside it relative to that.
    ("meter.x", Kind::X),
    ("meter.y", Kind::Y),
    ("left.x", Kind::XRel),
    ("left.y", Kind::YRel),
    ("right.x", Kind::XRel),
    ("right.y", Kind::YRel),
    ("mono.x", Kind::XRel),
    ("mono.y", Kind::YRel),
    ("left.origin.x", Kind::XRel),
    ("left.origin.y", Kind::YRel),
    ("right.origin.x", Kind::XRel),
    ("right.origin.y", Kind::YRel),
    ("mono.origin.x", Kind::XRel),
    ("mono.origin.y", Kind::YRel),
    ("flip.left.x", Kind::XRel),
    ("flip.right.x", Kind::XRel),
    ("distance", Kind::Len),
    ("step.width.regular", Kind::LenX),
    ("step.width.overload", Kind::LenX),
    ("bgr.filename", Kind::Picture),
    ("fgr.filename", Kind::Picture),
    ("indicator.filename", Kind::Picture),
    ("bar.filename", Kind::Picture),
    ("reflection.filename", Kind::Picture),
    ("bar.width", Kind::LenX),
    ("bar.height", Kind::LenY),
    ("bar.gap", Kind::LenX),
    ("bar.space", Kind::Share),
    ("led.space", Kind::Size),
    ("topping.height", Kind::LenY),
    ("topping.step", Kind::LenY),
    ("reflection.gap", Kind::LenY),
    // The spectrum box.
    ("spectrum.x", Kind::X),
    ("spectrum.y", Kind::Y),
    ("spectrum.size", Kind::Size),
    ("spectrum.*.size", Kind::Size),
    ("origin.x", Kind::XRel),
    ("origin.y", Kind::YRel),
    ("scale.size", Kind::Font),
    ("dot.size", Kind::Len),
    ("line.width", Kind::Len),
    ("line.width.max", Kind::Len),
    // Pictures and their boxes.
    ("albumart.pos", Kind::Point),
    ("albumart.dimension", Kind::Size),
    ("albumart.border", Kind::Len),
    ("albumart.mask", Kind::Picture),
    ("fanart.pos", Kind::Point),
    ("fanart.dimension", Kind::Size),
    ("vinyl.pos", Kind::Point),
    ("vinyl.dimension", Kind::Size),
    ("vinyl.center", Kind::Point),
    ("vinyl.filename", Kind::Picture),
    ("reel.*.pos", Kind::Point),
    ("reel.*.center", Kind::Point),
    ("reel.*.filename", Kind::Picture),
    ("tonearm.pivot.screen", Kind::Point),
    ("tonearm.pivot.image", Kind::Offset),
    ("tonearm.filename", Kind::Picture),
    // Text.
    ("font.size.*", Kind::Font),
    ("playinfo.type.dimension", Kind::Size),
    ("playinfo.type.fontsize", Kind::Font),
    ("playinfo.maxwidth", Kind::LenX),
    ("playinfo.**.maxwidth", Kind::LenX),
    ("playinfo.**.pos", Kind::Point),
    ("time.*.pos", Kind::Point),
    ("time.*.fontsize", Kind::Font),
    ("touch.margin", Kind::Len),
    // Controls and indicators, by their name.
    ("button.*.pos", Kind::Point),
    ("button.*.size", Kind::Size),
    ("button.*.image", Kind::Picture),
    ("*.pos", Kind::Point),
    ("*.dim", Kind::Size),
    ("*.font.size", Kind::Font),
    ("*.fill.offset", Kind::Offset),
    ("*.fill.radius", Kind::Len),
    ("*.fill.width", Kind::Len),
    ("*.arc.width", Kind::Len),
    ("*.head.offset", Kind::Offset),
    ("*.head.image", Kind::Picture),
    ("*.knob.image", Kind::Picture),
    ("*.slider.tip", Kind::Picture),
    ("*.slider.tip.offset", Kind::Offset),
    ("*.slider.track", Kind::Picture),
    ("*.slider.travel", Kind::Lens),
    ("*.icon", Kind::Picture),
    ("*.icon.glow", Kind::Len),
    ("*.led", Kind::Size),
    ("*.led.glow", Kind::Len),
    ("*.marker.*.pos", Kind::Offset),
    ("*.marker.*.fontsize", Kind::Font),
    ("*.marker.*.image", Kind::Picture),
    // The playinfo fields' other keys, and what does not scale.
    ("**.border", Kind::Len),
    ("**.dimension", Kind::Size),
    ("**.fontsize", Kind::Font),
    ("**.maxwidth", Kind::LenX),
    ("**.pos", Kind::Point),
];

/// Keys the parser reads whose values do not scale, named one by one so
/// the coverage test can hold the parser to this file.
const KEPT: &[&str] = &[
    // Keys of the previous engine the parser passes over.
    "ui.refresh.period",
    "steps.per.degree",
    "**.color",
    "**.files",
    "**.font",
    "**.scale",
    "**.zorder",
    "*.arc.angle.end",
    "*.arc.angle.start",
    "*.bg.color",
    "*.color",
    "*.fill.color",
    "*.icon.glow.color",
    "*.icon.glow.intensity",
    "*.knob.angle.end",
    "*.knob.angle.start",
    "*.led.color",
    "*.led.glow.color",
    "*.led.glow.intensity",
    "*.led.shape",
    "*.marker.*.label",
    "*.slider.orientation",
    "*.style",
    "albumart.rotation",
    "albumart.rotation.speed",
    "alpha",
    "bar.color",
    "bar.fade",
    "bar.glow",
    "bar.gradient",
    "bar.outline",
    "bar.round",
    "bar.type",
    "base.folder",
    "bgr.alpha",
    "bgr.color",
    "bgr.gradient",
    "bgr.type",
    "bins",
    "blend",
    "button.*.action",
    "channel",
    "channels",
    "color.mode",
    "config.extend",
    "current",
    "data.source",
    "direction",
    "dot.hold",
    "echo",
    "exit.on.touch",
    "fanart.scale",
    "fanart.zorder",
    "fill.alpha",
    "folderlayer.*",
    "folderlayer.enabled",
    "font.bold",
    "font.color",
    "font.digi",
    "font.italic",
    "font.light",
    "font.path",
    "font.regular",
    "frame.rate",
    "gravity",
    "image.extended",
    "indicator.type",
    "interactive",
    "layout",
    "led",
    "led.max",
    "led.true",
    "left.needle.flip",
    "left.start.angle",
    "left.stop.angle",
    "level.boost",
    "level.range",
    "level.scale",
    "line.glow",
    "lumi",
    "m",
    "max.value",
    "meter",
    "meter.folder",
    "meter.next",
    "meter.prev",
    "meter.previous",
    "meter.type",
    "meter.visible",
    "mirror",
    "mono",
    "mono.algorithm",
    "note.labels",
    "onset",
    "onset.color",
    "onset.decay",
    "onset.groups",
    "onset.strength",
    "palette",
    "palette.dir",
    "palette.left",
    "palette.right",
    "palette.split",
    "peaks",
    "peaks.fade",
    "peaks.hold",
    "peaks.line",
    "play.pause",
    "playinfo.align",
    "playinfo.center",
    "playinfo.scrolling.speed",
    "playinfo.scrolling.speed.*",
    "playinfo.text.center",
    "playinfo.ticker",
    "playinfo.ticker.direction",
    "playinfo.ticker.replace",
    "playinfo.ticker.separator",
    "playinfo.ticker.speed",
    "playinfo.type.align",
    "playinfo.type.label",
    "playinfo.type.mode",
    "position.fit",
    "position.overload",
    "position.regular",
    "position.type",
    "position.x",
    "position.y",
    "progress.border.color",
    "queue.mode",
    "radial",
    "radial.invert",
    "radius",
    "random",
    "random.change.title",
    "random.meter.interval",
    "range",
    "reel.direction",
    "reel.rotation.speed",
    "reflection.color",
    "reflection.gradient",
    "reflection.type",
    "reflex",
    "reflex.alpha",
    "reflex.bright",
    "reflex.fit",
    "right.needle.flip",
    "right.start.angle",
    "right.stop.angle",
    "rotation.fps",
    "rotation.quality",
    "rotation.speed",
    "scale",
    "scale.color",
    "scale.x",
    "scale.y",
    "scrolling.mode",
    "scrolling.speed.album",
    "scrolling.speed.artist",
    "scrolling.speed.title",
    "smooth.buffer.size",
    "smoothing",
    "sparkle",
    "spectrum.folder",
    "spectrum.name",
    "spectrum.visible",
    "spin",
    "spin.speed",
    "spool.adaptive",
    "spool.left.speed",
    "spool.right.speed",
    "start.angle",
    "start.animation",
    "steps",
    "stereo.algorithm",
    "stop.angle",
    "stop.display.on.touch",
    "style",
    "time.*.color",
    "time.*.font",
    "tonearm.angle.end",
    "tonearm.angle.rest",
    "tonearm.angle.start",
    "tonearm.drop.duration",
    "tonearm.lift.duration",
    "touch.interactive",
    "trail",
    "transition.color",
    "transition.duration",
    "transition.opacity",
    "transition.type",
    "use.system.fonts",
    "vinyl.direction",
    "volume.gain.db",
    "volume.gain.db.source",
    "volume.max",
    "volume.max.in.pipe",
    "volume.value",
    "waterfall.reverse",
    "waterfall.speed",
    "weighting",
    "window",
];

/// Whether a pattern of segments matches a key's segments.
fn matches(pattern: &str, key: &str) -> bool {
    fn go(p: &[&str], k: &[&str]) -> bool {
        match (p.first(), k.first()) {
            (None, None) => true,
            (Some(&"**"), Some(_)) => (1..=k.len()).any(|n| go(&p[1..], &k[n..])),
            (Some(&"*"), Some(_)) => go(&p[1..], &k[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &k[1..]),
            _ => false,
        }
    }
    let p: Vec<&str> = pattern.split('.').collect();
    let k: Vec<&str> = key.split('.').collect();
    go(&p, &k)
}

/// The kind of a key: from the table, or kept when the table names it so;
/// `None` for a key this file does not know.
pub fn kind_of(key: &str) -> Option<Kind> {
    let key = key.trim().to_ascii_lowercase();
    if let Some((_, kind)) = RULES.iter().find(|(p, _)| matches(p, &key)) {
        return Some(*kind);
    }
    if KEPT.iter().any(|p| matches(p, &key)) {
        return Some(Kind::Keep);
    }
    None
}

/// A tailored text: the new text, the pictures it names, and what could
/// not be scaled.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tailored {
    pub text: String,
    pub pictures: Vec<String>,
    /// Those among the pictures that are the screen's own, to be padded to the new screen.
    pub backdrops: Vec<String>,
    pub warnings: Vec<String>,
}

/// A number as the theme wrote it, whole or with a point, scaled and
/// written back the same way.
fn scaled(value: &str, factor: f32, offset: f32) -> Option<String> {
    let v = value.trim();
    let n: f32 = v.parse().ok()?;
    let out = n * factor + offset;
    if v.contains('.') {
        Some(
            format!("{:.2}", out)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string(),
        )
    } else {
        Some(format!("{}", out.round() as i64))
    }
}

/// A value's fields, split on commas and trimmed.
fn fields(value: &str) -> Vec<String> {
    value.split(',').map(|f| f.trim().to_string()).collect()
}

/// The value of a key as the cutter writes it, or `None` to keep it.
fn transform(key: &str, kind: Kind, value: &str, plan: &Plan) -> Result<Option<String>, String> {
    // An empty value says the key is unset: it stays so.
    if value.trim().is_empty() {
        return Ok(None);
    }
    let two = |fx: f32, fy: f32, ox: f32, oy: f32| -> Result<Option<String>, String> {
        let mut f = fields(value);
        if f.len() < 2 {
            return Err(format!(
                "{key}: two numbers expected, found `{}`",
                value.trim()
            ));
        }
        let x =
            scaled(&f[0], fx, ox).ok_or_else(|| format!("{key}: `{}` is not a number", f[0]))?;
        let y =
            scaled(&f[1], fy, oy).ok_or_else(|| format!("{key}: `{}` is not a number", f[1]))?;
        f[0] = x;
        f[1] = y;
        Ok(Some(f.join(",")))
    };
    let one = |factor: f32, offset: f32| -> Result<Option<String>, String> {
        scaled(value, factor, offset)
            .map(Some)
            .ok_or_else(|| format!("{key}: `{}` is not a number", value.trim()))
    };
    match kind {
        Kind::Point => two(plan.sx, plan.sy, plan.ox, plan.oy),
        Kind::Offset => two(plan.sx, plan.sy, 0.0, 0.0),
        Kind::Size => two(plan.sx, plan.sy, 0.0, 0.0),
        Kind::Lens => two(plan.s(), plan.s(), 0.0, 0.0),
        Kind::X => one(plan.sx, plan.ox),
        Kind::Y => one(plan.sy, plan.oy),
        Kind::XRel | Kind::LenX => one(plan.sx, 0.0),
        Kind::YRel | Kind::LenY => one(plan.sy, 0.0),
        Kind::Len | Kind::Font => one(plan.s(), 0.0),
        Kind::Share => {
            let n: f32 = value
                .trim()
                .parse()
                .map_err(|_| format!("{key}: `{}` is not a number", value.trim()))?;
            if n >= 1.0 {
                one(plan.sx, 0.0)
            } else {
                Ok(None)
            }
        }
        Kind::Screen => Ok(Some(
            if key.ends_with("width") {
                plan.to.0
            } else {
                plan.to.1
            }
            .to_string(),
        )),
        Kind::Picture | Kind::Backdrop | Kind::Keep => Ok(None),
    }
}

/// `name` on the list, once.
fn noted(list: &mut Vec<String>, name: &str) {
    if !list.iter().any(|p| p == name) {
        list.push(name.to_string());
    }
}

/// A theme text scaled by the plan, line by line: the keys' values
/// rewritten by their kind, everything else as it was, the line endings
/// kept. Pictures the text names are listed for resampling; a key the
/// table does not know, or a value that is not what its kind expects,
/// is left as it is and named in the warnings.
pub fn tailor_text(text: &str, plan: &Plan) -> Tailored {
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + 64);
    let mut pictures: Vec<String> = Vec::new();
    let mut backdrops: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    let mut section = String::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed.to_string();
        }
        let rewritten =
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
                None
            } else if let Some(eq) = line.find('=') {
                let key = line[..eq].trim();
                let value = &line[eq + 1..];
                match kind_of(key) {
                    None => {
                        warnings.push(format!(
                            "{section} {key}: not a key the cutter knows; kept as it is"
                        ));
                        None
                    }
                    Some(Kind::Picture) => {
                        for name in value.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                            noted(&mut pictures, name);
                        }
                        None
                    }
                    Some(Kind::Backdrop) => {
                        for name in value.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                            noted(&mut pictures, name);
                            noted(&mut backdrops, name);
                        }
                        None
                    }
                    Some(kind) => match transform(key, kind, value, plan) {
                        Ok(Some(new)) => Some(format!("{} = {}", line[..eq].trim_end(), new)),
                        Ok(None) => None,
                        Err(why) => {
                            warnings.push(format!("{section} {why}; kept as it is"));
                            None
                        }
                    },
                }
            } else {
                None
            };
        out.push_str(rewritten.as_deref().unwrap_or(line));
        out.push_str(ending);
    }
    // The split gave one line more than the endings between them: one
    // ending too many was written, whether the text ended with one or not.
    out.truncate(out.len() - ending.len());
    Tailored {
        text: out,
        pictures,
        backdrops,
        warnings,
    }
}

/// The name a tailored theme takes: the size in front, the rest of the
/// name kept, `1024x600_amber` to `1280x720_amber`, a name without a
/// size given one.
pub fn tailored_name(name: &str, to: (u32, u32)) -> String {
    let is_size = |s: &str| {
        s.split_once('x')
            .is_some_and(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok())
    };
    let rest = match name.split_once('_') {
        Some((size, rest)) if is_size(size) => rest,
        None if is_size(name) => "",
        _ => name,
    };
    if rest.is_empty() {
        format!("{}x{}", to.0, to.1)
    } else {
        format!("{}x{}_{}", to.0, to.1, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parser's own source, held to the table: every key it reads by
    /// a helper, a match arm or a comparison, and every dotted literal,
    /// is a key the table names or a word this list says is a value.
    #[test]
    fn every_key_the_parser_reads_is_classified() {
        const VALUES: &[&str] = &[
            "a",
            "airplay",
            "all",
            "area",
            "artist",
            "aurora",
            "b",
            "back.jpg",
            "back.png",
            "bars",
            "bass",
            "bluetooth",
            "bold",
            "button.",
            "c",
            "ccw",
            "cd",
            "cdart.png",
            "center",
            "classic",
            "combined",
            "custom",
            "cw",
            "d",
            "dab",
            "default",
            "digi",
            "dismiss",
            "dlna",
            "dots",
            "dsf",
            "ember",
            "exit",
            "false",
            "fit",
            "flash",
            "fm",
            "graph",
            "high",
            "horizontal",
            "ice",
            "index",
            "italic",
            "itu468",
            "left",
            "level",
            "light",
            "line",
            "list",
            "logo.png",
            "logo.txt",
            "low",
            "mid",
            "mnt",
            "mute",
            "next",
            "none",
            "off",
            "on",
            "orangered",
            "pause",
            "play",
            "playpause",
            "prev",
            "previous",
            "prism",
            "progress",
            "pulse",
            "qobuz",
            "radio",
            "rainbow",
            "reel.png",
            "regular",
            "repeat",
            "right",
            "ring",
            "rtlsdr",
            "s.2",
            "s.9",
            "shuffle",
            "spectrogram",
            "spotify",
            "steelblue",
            "stop",
            "sub",
            "subbass",
            "tidal",
            "title",
            "toggle",
            "true",
            "upnp",
            "vertical",
            "vinyl.jpg",
            "violet",
            "volume",
            "waterfall",
            "webradio",
        ];
        let src = include_str!("lib.rs");
        let mut keys: Vec<String> = Vec::new();
        let mut push = |k: &str| {
            let k = k.replace("{prefix}", "**");
            let mut k = k.to_string();
            while let Some(a) = k.find('{') {
                let b = k[a..].find('}').map(|b| a + b + 1).unwrap_or(k.len());
                k.replace_range(a..b, "*");
            }
            if k.contains(' ') || k.contains('\\') {
                return;
            }
            if !keys.contains(&k) {
                keys.push(k);
            }
        };
        let literal = |s: &str| -> Option<(String, usize)> {
            let end = s.find('"')?;
            let lit = &s[..end];
            if lit.is_empty()
                || !lit.bytes().all(|b| {
                    b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'.'
                        || b == b'{'
                        || b == b'}'
                })
                || !lit.as_bytes()[0].is_ascii_lowercase()
            {
                return None;
            }
            Some((lit.to_string(), end))
        };
        for helper in [
            "get(\"",
            "number(\"",
            "flag(\"",
            "uint(\"",
            "signed(\"",
            "int(\"",
            "pair(\"",
            "ipair(\"",
            "upair(\"",
            "style(\"",
            "current_value(text, \"",
            "section_value(text, \"",
            "format!(\"",
        ] {
            for (at, _) in src.match_indices(helper) {
                if let Some((lit, _)) = literal(&src[at + helper.len()..]) {
                    if helper == "format!(\"" && !lit.contains('{') {
                        continue;
                    }
                    push(&lit);
                }
            }
        }
        // Every dotted literal, and every literal a match arm or a comparison names.
        let mut rest = src;
        while let Some(a) = rest.find('"') {
            let after = &rest[a + 1..];
            if let Some((lit, end)) = literal(after) {
                let tail = after[end + 1..].trim_start();
                let head = rest[..a].trim_end();
                let named = tail.starts_with("=>")
                    || tail.starts_with('|')
                    || head.ends_with("==")
                    || head.ends_with("!=")
                    || head.ends_with("starts_with(")
                    || head.ends_with("strip_prefix(");
                if lit.contains('.') || named {
                    push(&lit);
                }
                rest = &after[end + 1..];
            } else {
                rest = after;
            }
        }
        let unknown: Vec<&String> = keys
            .iter()
            .filter(|k| kind_of(k).is_none() && !VALUES.contains(&k.as_str()))
            .collect();
        assert!(
            unknown.is_empty(),
            "keys the cutter does not know: {unknown:?}"
        );
        assert!(
            keys.len() > 250,
            "the extraction found only {} keys",
            keys.len()
        );
    }

    #[test]
    fn a_fitted_plan_keeps_the_shape_and_centres_and_a_stretched_one_fills() {
        let fit = Plan::fit((1024, 600), (1280, 720));
        assert!((fit.sx - 1.2).abs() < 1e-6 && fit.sx == fit.sy);
        assert_eq!((fit.ox, fit.oy), (25.0, 0.0));
        let stretch = Plan::stretch((1024, 600), (1280, 720));
        assert!((stretch.sx - 1.25).abs() < 1e-6 && (stretch.sy - 1.2).abs() < 1e-6);
        assert_eq!((stretch.ox, stretch.oy), (0.0, 0.0));
        assert_eq!(
            tailored_name("1024x600_amber", (1280, 720)),
            "1280x720_amber"
        );
        assert_eq!(tailored_name("amber", (1280, 720)), "1280x720_amber");
        assert_eq!(tailored_name("1024x600", (1280, 720)), "1280x720");
    }

    /// A meter text through the plan: screen points offset, meter-relative
    /// ones only scaled, sizes and fonts scaled, pictures listed, a share
    /// kept and pixels scaled, the unknown named, comments and endings kept.
    #[test]
    fn the_text_is_rewritten_by_the_kind_of_each_key() {
        let plan = Plan::fit((1000, 500), (2000, 1200));
        assert_eq!((plan.sx, plan.ox, plan.oy), (2.0, 0.0, 100.0));
        let text = "# a comment\r\n[gold]\r\nmeter.type = circular\r\nmeter.x = 10\r\nmeter.y = 20\r\nleft.x = 5\r\nleft.origin.y = 7\r\ndistance = 30\r\nbgr.filename = gold-bgr.png\r\nalbumart.pos = 100,50\r\nalbumart.dimension = 40,40\r\nplayinfo.title.pos = 10,20,bold\r\nfont.size.bold = 16\r\nvolume.pos = 1,2\r\nvolume.dim = 3,4\r\nvolume.slider.travel = 5,9\r\nvolume.marker.1.pos = 6,6\r\nbutton.play.image = play.png, play-lit.png\r\nvinyl.pos = 10,10\r\nvinyl.center = 20,30\r\nreel.left.center = 5,5\r\ntonearm.pivot.image = 3,4\r\nscreen.bgr = wall.jpg\r\nstart.angle = 45\r\nbar.space = 0.25\r\nscreen.width = 1000\r\nmystery.key = 4\r\nleft.y = oops\r\n";
        let t = tailor_text(text, &plan);
        let want = "# a comment\r\n[gold]\r\nmeter.type = circular\r\nmeter.x = 20\r\nmeter.y = 140\r\nleft.x = 10\r\nleft.origin.y = 14\r\ndistance = 60\r\nbgr.filename = gold-bgr.png\r\nalbumart.pos = 200,200\r\nalbumart.dimension = 80,80\r\nplayinfo.title.pos = 20,140,bold\r\nfont.size.bold = 32\r\nvolume.pos = 2,104\r\nvolume.dim = 6,8\r\nvolume.slider.travel = 10,18\r\nvolume.marker.1.pos = 12,12\r\nbutton.play.image = play.png, play-lit.png\r\nvinyl.pos = 20,120\r\nvinyl.center = 40,160\r\nreel.left.center = 10,110\r\ntonearm.pivot.image = 6,8\r\nscreen.bgr = wall.jpg\r\nstart.angle = 45\r\nbar.space = 0.25\r\nscreen.width = 2000\r\nmystery.key = 4\r\nleft.y = oops\r\n";
        assert_eq!(t.text, want);
        assert_eq!(
            t.pictures,
            vec!["gold-bgr.png", "play.png", "play-lit.png", "wall.jpg"]
        );
        assert_eq!(t.backdrops, vec!["wall.jpg"]);
        assert_eq!(t.warnings.len(), 2, "{:?}", t.warnings);
        assert!(t.warnings[0].contains("mystery.key") && t.warnings[1].contains("left.y"));
        let spaced = tailor_text("[s]\nbar.space = 3\norigin.x = 2.5\n", &plan);
        assert_eq!(spaced.text, "[s]\nbar.space = 6\norigin.x = 5\n");
        assert_eq!(
            tailor_text("[s]\nmeter.x = 1", &plan).text,
            "[s]\nmeter.x = 2"
        );
    }
}
