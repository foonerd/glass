//! What a display brings from a player's manager, and how the player's
//! configuration is rewritten for a home of the display's own: the same
//! for a remote display on a machine, which fetches into a directory, and
//! for a page in a browser, which fetches into the file table. Nothing here
//! touches the network or a disk.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A font or an icon the manager lists, with its checksum.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub bytes: u64,
}

/// What the manager answers at `/api/remote/config`: the configuration
/// version, the theme and meter on show, the two configuration files as
/// text, and the assets every theme draws with.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RemoteConfig {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub meter: String,
    #[serde(default)]
    pub files: RemoteFiles,
    #[serde(default)]
    pub assets: RemoteAssets,
    /// What a display with a face needs of the player's; empty from a
    /// player older than 0.8.20.
    #[serde(default)]
    pub face: RemoteFace,
}

/// Whose the player's screen is (`glass-evo` where its face holds it), and
/// the look the player's settings name, as text.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RemoteFace {
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub theme: Option<FaceTheme>,
}

/// A face theme by its name and its `face.txt`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FaceTheme {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub text: String,
}

/// Whether a face theme's name is one folder's name and nothing else.
pub fn face_theme_name_ok(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RemoteFiles {
    #[serde(default)]
    pub meter: String,
    #[serde(default)]
    pub spectrum: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct RemoteAssets {
    #[serde(default)]
    pub fonts: Vec<Asset>,
    #[serde(default)]
    pub icons: Vec<Asset>,
    /// The player's web fonts, the ones its configuration's `font.path`
    /// names; a player older than Glass 0.7.14 lists none.
    #[serde(default)]
    pub webfonts: Vec<Asset>,
    /// Fonts the listener uploaded to the player, named by path in its
    /// configuration; a player older than Glass 0.7.27 lists none.
    #[serde(default)]
    pub custom: Vec<Asset>,
}

/// What the manager answers at `/api/themes/<folder>/files`: the theme's
/// files with their checksums, and the spectrum twin's when there is one.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ThemeFiles {
    pub folder: String,
    #[serde(default)]
    pub files: Vec<ThemeFile>,
    #[serde(default)]
    pub spectrum: Option<ThemeTree>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ThemeTree {
    pub folder: String,
    #[serde(default)]
    pub files: Vec<ThemeFile>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ThemeFile {
    pub path: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub bytes: u64,
}

/// What a display wants of the player's configuration: the player's own
/// theme and meter when nothing is set, or a theme of its choosing from
/// the player's themes with a meter choice of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    /// A theme folder of the player's to bring instead of the one on show,
    /// or, with `local` set, a theme folder under the local folders.
    pub theme: Option<String>,
    /// The `meter` value: a name, a comma list, or `random`.
    pub meter: Option<String>,
    /// Seconds between meters when they rotate, 15 to 1000.
    pub interval_s: Option<u32>,
    /// Whether a new title moves to the next meter.
    pub on_title: Option<bool>,
    /// Themes on the display's own machine: the configuration is pointed
    /// at these folders and no theme is brought from the player.
    pub local: Option<LocalThemes>,
}

/// The folders themes are read from on the display's own machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalThemes {
    pub templates: PathBuf,
    pub spectrum: PathBuf,
}

/// The two configuration files as they are written into a home, and the
/// theme they name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Texts {
    pub meter: String,
    pub spectrum: String,
    pub theme: String,
}

/// A file to bring: where it lands under the home, the manager's path
/// that serves it, and the checksum it must have, empty for any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bring {
    pub relative: String,
    pub url: String,
    pub sha256: String,
}

/// The player's configuration files rewritten for `home`: the theme
/// folders under it (or the local folders the choice names), its web
/// fonts under `webfonts`, an uploaded font at its copy under
/// `customfonts`, and the theme, meter and rotation the choice asks for.
pub fn config_texts(config: &RemoteConfig, home: &Path, choice: &Choice) -> Texts {
    let (templates, spectrum_templates) = match &choice.local {
        Some(local) => (local.templates.clone(), local.spectrum.clone()),
        None => (home.join("templates"), home.join("templates_spectrum")),
    };
    let webfonts = home.join("webfonts");
    let theme = choice
        .theme
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(&config.theme)
        .to_string();
    let templates = templates.to_string_lossy().into_owned();
    let webfonts = webfonts.to_string_lossy().into_owned();
    let interval = choice.interval_s.map(|i| i.clamp(15, 1000).to_string());
    let on_title = choice.on_title.map(|t| if t { "True" } else { "False" });
    // A style set in an uploaded font names it by its path on the player;
    // here it is the copy brought into this home.
    let custom_dir = home.join("customfonts");
    let custom_names: Vec<String> = config
        .assets
        .custom
        .iter()
        .map(|f| f.name.clone())
        .collect();
    let custom_keys = custom_font_keys(&config.files.meter, &custom_dir, &custom_names);
    let mut keys: Vec<(&str, &str)> = vec![
        ("base.folder", &templates),
        ("font.path", &webfonts),
        ("meter.folder", &theme),
    ];
    for (key, value) in &custom_keys {
        keys.push((key.as_str(), value.as_str()));
    }
    if let Some(meter) = choice.meter.as_deref().filter(|m| !m.trim().is_empty()) {
        keys.push(("meter", meter));
    }
    if let Some(interval) = interval.as_deref() {
        keys.push(("random.meter.interval", interval));
    }
    if let Some(on_title) = on_title {
        keys.push(("random.change.title", on_title));
    }
    let meter = rewrite_config(&config.files.meter, &keys);
    let spectrum_templates = spectrum_templates.to_string_lossy().into_owned();
    let spectrum = rewrite_config(
        &config.files.spectrum,
        &[
            ("base.folder", &spectrum_templates),
            ("spectrum.folder", &theme),
        ],
    );
    Texts {
        meter,
        spectrum,
        theme,
    }
}

/// The fonts and icons to bring for a configuration: the plugin's fonts,
/// its icons and the player's, the web fonts, the uploaded fonts, each
/// under its own folder of the home.
pub fn asset_plan(config: &RemoteConfig) -> Vec<Bring> {
    let mut plan = Vec::new();
    let mut add = |assets: &[Asset], kind: &str, folder: &str| {
        for asset in assets {
            plan.push(Bring {
                relative: format!("{folder}/{}", asset.name),
                url: format!("/api/remote/asset/{kind}/{}", encode(&asset.name)),
                sha256: asset.sha256.clone(),
            });
        }
    };
    add(&config.assets.fonts, "font", "fonts");
    add(&config.assets.icons, "icon", "format-icons");
    add(&config.assets.webfonts, "webfont", "webfonts");
    add(&config.assets.custom, "custom", "customfonts");
    plan
}

/// The files of a theme to bring, the spectrum twin's included, under
/// `templates/<folder>/` and `templates_spectrum/<folder>/`.
pub fn theme_plan(theme: &ThemeFiles) -> Vec<Bring> {
    let mut plan: Vec<Bring> = theme
        .files
        .iter()
        .map(|file| Bring {
            relative: format!("templates/{}/{}", theme.folder, file.path),
            url: format!(
                "/api/themes/{}/file?tree=templates&path={}",
                encode(&theme.folder),
                encode(&file.path)
            ),
            sha256: file.sha256.clone(),
        })
        .collect();
    if let Some(spectrum) = &theme.spectrum {
        plan.extend(spectrum.files.iter().map(|file| Bring {
            relative: format!("templates_spectrum/{}/{}", spectrum.folder, file.path),
            url: format!(
                "/api/themes/{}/file?tree=templates_spectrum&path={}",
                encode(&spectrum.folder),
                encode(&file.path)
            ),
            sha256: file.sha256.clone(),
        }));
    }
    plan
}

/// The `font.<style>` keys whose value names an uploaded font by path,
/// each with the path of that font's copy under `dir`, by file name.
pub fn custom_font_keys(text: &str, dir: &Path, names: &[String]) -> Vec<(String, String)> {
    [
        "font.light",
        "font.regular",
        "font.bold",
        "font.italic",
        "font.digi",
    ]
    .iter()
    .filter_map(|key| {
        let value = lead::current_value(text, key)?;
        let value = value.trim();
        let name = Path::new(value).file_name()?.to_str()?;
        if value.starts_with('/') && names.iter().any(|n| n == name) {
            Some((
                (*key).to_string(),
                dir.join(name).to_string_lossy().into_owned(),
            ))
        } else {
            None
        }
    })
    .collect()
}

/// `key = value` lines at the top level of a configuration, the given keys
/// given new values; a key not there is added under `[current]`.
pub fn rewrite_config(text: &str, keys: &[(&str, &str)]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let mut replaced = None;
        for (key, value) in keys {
            if let Some(rest) = trimmed.strip_prefix(key) {
                if rest.trim_start().starts_with('=') {
                    replaced = Some(format!("{key} = {value}"));
                    seen.push(key);
                    break;
                }
            }
        }
        out.push(replaced.unwrap_or_else(|| line.to_string()));
    }
    for (key, value) in keys {
        if !seen.contains(key) {
            let at = out
                .iter()
                .position(|l| l.trim().eq_ignore_ascii_case("[current]"))
                .map(|i| i + 1)
                .unwrap_or(out.len());
            out.insert(at, format!("{key} = {value}"));
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

/// Percent-encoding for a path segment or a query value.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_themes_name_is_one_folders_name() {
        for good in ["Dark Glass", "Night Drive", "my_look-2 (v1.0)"] {
            assert!(face_theme_name_ok(good), "{good}");
        }
        for bad in [
            "",
            "  ",
            ".hidden",
            "../out",
            "a/b",
            "a\\b",
            &"x".repeat(65),
        ] {
            assert!(!face_theme_name_ok(bad), "{bad:?}");
        }
    }

    fn config() -> RemoteConfig {
        RemoteConfig {
            version: "abc".into(),
            theme: "1280x720_Deck".into(),
            meter: "random".into(),
            files: RemoteFiles {
                meter: "[current]\nbase.folder = /data/INTERNAL/glass/templates\nmeter.folder = 1280x720_Deck\nmeter = random\nfont.path = /volumio/fonts\nfont.bold = /data/INTERNAL/glass/fonts/Mine.ttf\n".into(),
                spectrum: "[current]\nbase.folder = /data/INTERNAL/glass/templates_spectrum\nspectrum.folder = 1280x720_Deck\n".into(),
            },
            assets: RemoteAssets {
                fonts: vec![Asset { name: "DSEG7.ttf".into(), sha256: "f1".into(), bytes: 1 }],
                icons: vec![Asset { name: "flac.svg".into(), sha256: "i1".into(), bytes: 1 }],
                webfonts: vec![Asset { name: "Lato Bold.ttf".into(), sha256: "w1".into(), bytes: 1 }],
                custom: vec![Asset { name: "Mine.ttf".into(), sha256: "c1".into(), bytes: 1 }],
            },
            face: RemoteFace::default(),
        }
    }

    #[test]
    fn the_configuration_points_into_the_home() {
        let texts = config_texts(&config(), Path::new("/glass"), &Choice::default());
        assert_eq!(texts.theme, "1280x720_Deck");
        assert!(texts.meter.contains("base.folder = /glass/templates\n"));
        assert!(texts.meter.contains("font.path = /glass/webfonts\n"));
        assert!(texts
            .meter
            .contains("font.bold = /glass/customfonts/Mine.ttf\n"));
        assert!(texts.meter.contains("meter.folder = 1280x720_Deck\n"));
        assert!(texts
            .spectrum
            .contains("base.folder = /glass/templates_spectrum\n"));
    }

    #[test]
    fn a_choice_names_the_theme_the_meter_and_the_rotation() {
        let choice = Choice {
            theme: Some("800x480_Other".into()),
            meter: Some("left,right".into()),
            interval_s: Some(5),
            on_title: Some(true),
            local: None,
        };
        let texts = config_texts(&config(), Path::new("/glass"), &choice);
        assert_eq!(texts.theme, "800x480_Other");
        assert!(texts.meter.contains("meter = left,right\n"));
        assert!(texts.meter.contains("random.meter.interval = 15\n"));
        assert!(texts.meter.contains("random.change.title = True\n"));
        assert!(texts.spectrum.contains("spectrum.folder = 800x480_Other\n"));
    }

    #[test]
    fn local_folders_replace_the_home_s() {
        let choice = Choice {
            local: Some(LocalThemes {
                templates: PathBuf::from("/mnt/share/templates"),
                spectrum: PathBuf::from("/mnt/share/templates_spectrum"),
            }),
            ..Choice::default()
        };
        let texts = config_texts(&config(), Path::new("/glass"), &choice);
        assert!(texts.meter.contains("base.folder = /mnt/share/templates\n"));
        assert!(texts
            .spectrum
            .contains("base.folder = /mnt/share/templates_spectrum\n"));
    }

    #[test]
    fn the_plans_name_every_file_once_with_its_place_and_route() {
        let plan = asset_plan(&config());
        let relative: Vec<&str> = plan.iter().map(|b| b.relative.as_str()).collect();
        assert_eq!(
            relative,
            [
                "fonts/DSEG7.ttf",
                "format-icons/flac.svg",
                "webfonts/Lato Bold.ttf",
                "customfonts/Mine.ttf"
            ]
        );
        assert_eq!(plan[2].url, "/api/remote/asset/webfont/Lato%20Bold.ttf");
        assert_eq!(plan[3].sha256, "c1");
        let theme = ThemeFiles {
            folder: "1280x720_Deck".into(),
            files: vec![ThemeFile {
                path: "meters.txt".into(),
                sha256: "m".into(),
                bytes: 1,
            }],
            spectrum: Some(ThemeTree {
                folder: "1280x720_Deck".into(),
                files: vec![ThemeFile {
                    path: "bar/one.png".into(),
                    sha256: "s".into(),
                    bytes: 1,
                }],
            }),
        };
        let plan = theme_plan(&theme);
        assert_eq!(plan[0].relative, "templates/1280x720_Deck/meters.txt");
        assert_eq!(
            plan[0].url,
            "/api/themes/1280x720_Deck/file?tree=templates&path=meters.txt"
        );
        assert_eq!(
            plan[1].relative,
            "templates_spectrum/1280x720_Deck/bar/one.png"
        );
        assert_eq!(
            plan[1].url,
            "/api/themes/1280x720_Deck/file?tree=templates_spectrum&path=bar%2Fone.png"
        );
    }
}
