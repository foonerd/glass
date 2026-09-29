//! The cutter as a command: a theme folder and its spectrum twin scaled
//! to another screen and written as a new theme pair, `glass --tailor`.
//! The text goes through lead's table; every picture in the folders is
//! resampled by the same plan; everything else is copied as it is.

use std::fs;
use std::path::{Path, PathBuf};

use lead::tailor::{tailor_text, tailored_name, Plan};

/// What to cut: the theme folder, its spectrum twin when it has one, the
/// size it was made for when its name does not say, the size to cut to,
/// where the pair goes, and whether to stretch instead of fit.
pub struct Job {
    pub theme: PathBuf,
    pub spectrum: Option<PathBuf>,
    pub from: Option<(u32, u32)>,
    pub to: (u32, u32),
    pub out: PathBuf,
    pub stretch: bool,
}

/// What was done: the folders written, the pictures resampled, and what
/// the cutter had to leave as it was.
#[derive(Debug, Default)]
pub struct Report {
    pub name: String,
    pub folders: Vec<PathBuf>,
    pub pictures: usize,
    pub warnings: Vec<String>,
}

/// `WxH` as a size.
pub fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once('x')?;
    let (w, h) = (w.parse::<u32>().ok()?, h.parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// The size a theme was made for: the `WxH` its folder name starts
/// with, else `screen.width` and `screen.height` in its meters file.
fn size_of(name: &str, meters: &str) -> Option<(u32, u32)> {
    if let Some(size) = name.split('_').next().and_then(parse_size) {
        return Some(size);
    }
    let mut width = None;
    let mut height = None;
    for line in meters.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "screen.width" => width = value.trim().parse::<u32>().ok(),
                "screen.height" => height = value.trim().parse::<u32>().ok(),
                _ => {}
            }
        }
    }
    Some((width?, height?))
}

fn is_picture(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp")
    )
}

/// One folder through the plan into `out`: the text file named scaled,
/// pictures resampled, the rest copied, folders inside the same.
fn cut_folder(
    src: &Path,
    out: &Path,
    text_file: &str,
    plan: &Plan,
    report: &mut Report,
) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut entries: Vec<_> = fs::read_dir(src)
        .map_err(|e| format!("{}: {e}", src.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let dst = out.join(&name);
        if path.is_dir() {
            cut_folder(&path, &dst, "", plan, report)?;
        } else if !text_file.is_empty() && name_str == text_file {
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let tailored = tailor_text(&text, plan);
            fs::write(&dst, tailored.text).map_err(|e| format!("{}: {e}", dst.display()))?;
            for warning in tailored.warnings {
                report.warnings.push(format!("{}: {warning}", name_str));
            }
            for picture in tailored.pictures {
                if !src.join(&picture).is_file() {
                    report.warnings.push(format!(
                        "{}: names {picture}, which the folder does not hold",
                        name_str
                    ));
                }
            }
        } else if is_picture(&path) {
            match expose::resample_picture(&path, &dst, plan.sx, plan.sy) {
                Ok(_) => report.pictures += 1,
                Err(why) => {
                    report.warnings.push(format!("{why}; copied as it is"));
                    fs::copy(&path, &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
                }
            }
        } else {
            fs::copy(&path, &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
    }
    Ok(())
}

/// Cut the job: `out/templates/<name>` and, with a twin, `out/templates_spectrum/<name>`.
pub fn run(job: &Job) -> Result<Report, String> {
    let theme_name = job
        .theme
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{}: not a theme folder", job.theme.display()))?
        .to_string();
    let meters_path = job.theme.join("meters.txt");
    let meters =
        fs::read_to_string(&meters_path).map_err(|e| format!("{}: {e}", meters_path.display()))?;
    let from = job
        .from
        .or_else(|| size_of(&theme_name, &meters))
        .ok_or_else(|| {
            "the theme's size is not known: name its folder WxH_name, or say --from WxH".to_string()
        })?;
    let plan = if job.stretch {
        Plan::stretch(from, job.to)
    } else {
        Plan::fit(from, job.to)
    };
    let name = tailored_name(&theme_name, job.to);
    let mut report = Report {
        name: name.clone(),
        ..Report::default()
    };
    let meters_out = job.out.join("templates").join(&name);
    if meters_out.exists() {
        return Err(format!("{}: exists already", meters_out.display()));
    }
    cut_folder(&job.theme, &meters_out, "meters.txt", &plan, &mut report)?;
    report.folders.push(meters_out);
    let twin = job.spectrum.clone().or_else(|| {
        let beside = job
            .theme
            .parent()?
            .parent()?
            .join("templates_spectrum")
            .join(&theme_name);
        beside.join("spectrum.txt").is_file().then_some(beside)
    });
    if let Some(twin) = twin {
        let spectrum_out = job.out.join("templates_spectrum").join(&name);
        if spectrum_out.exists() {
            return Err(format!("{}: exists already", spectrum_out.display()));
        }
        cut_folder(&twin, &spectrum_out, "spectrum.txt", &plan, &mut report)?;
        report.folders.push(spectrum_out);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small theme with a twin through the cutter: the pair written under
    /// the new name, the text scaled, a picture resampled to the new size,
    /// a font copied as it is, and a picture the text names but the folder
    /// lacks reported.
    #[test]
    fn a_theme_and_its_twin_are_cut_to_the_new_size() {
        let root = std::env::temp_dir().join(format!("glass-tailor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let theme = root.join("templates").join("100x50_tiny");
        let twin = root.join("templates_spectrum").join("100x50_tiny");
        fs::create_dir_all(theme.join("fonts")).unwrap();
        fs::create_dir_all(&twin).unwrap();
        fs::write(
            theme.join("meters.txt"),
            "[m]\nmeter.x = 10\nmeter.y = 5\nbgr.filename = bgr.png\nindicator.filename = gone.png\nleft.x = 4\n",
        )
        .unwrap();
        fs::write(theme.join("fonts").join("a.ttf"), b"font").unwrap();
        let picture = expose::Frame {
            blend: expose::Blend::Normal,
            width: 20,
            height: 10,
            rgba: [200, 100, 50, 255].repeat(200),
        };
        expose::write_png(&theme.join("bgr.png"), &picture).unwrap();
        fs::write(
            twin.join("spectrum.txt"),
            "[s]\nspectrum.x = 1\nspectrum.y = 2\nbar.width = 3\n",
        )
        .unwrap();
        let out = root.join("out");
        let report = run(&Job {
            theme: theme.clone(),
            spectrum: None,
            from: None,
            to: (200, 120),
            out: out.clone(),
            stretch: false,
        })
        .expect("cut");
        assert_eq!(report.name, "200x120_tiny");
        assert_eq!(report.pictures, 1);
        let meters = fs::read_to_string(out.join("templates/200x120_tiny/meters.txt")).unwrap();
        // The factor is 2 (100 to 200, and 50 to 100 within 120), the theme centred: y offset 10.
        assert_eq!(
            meters,
            "[m]\nmeter.x = 20\nmeter.y = 20\nbgr.filename = bgr.png\nindicator.filename = gone.png\nleft.x = 8\n"
        );
        let spectrum =
            fs::read_to_string(out.join("templates_spectrum/200x120_tiny/spectrum.txt")).unwrap();
        assert_eq!(
            spectrum,
            "[s]\nspectrum.x = 2\nspectrum.y = 14\nbar.width = 6\n"
        );
        assert_eq!(
            expose::picture_size(&out.join("templates/200x120_tiny/bgr.png")),
            Some((40, 20))
        );
        assert_eq!(
            fs::read(out.join("templates/200x120_tiny/fonts/a.ttf")).unwrap(),
            b"font"
        );
        assert!(
            report.warnings.iter().any(|w| w.contains("gone.png")),
            "{:?}",
            report.warnings
        );
        assert!(
            run(&Job {
                theme,
                spectrum: None,
                from: None,
                to: (200, 120),
                out,
                stretch: false,
            })
            .is_err(),
            "a second cut does not overwrite"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
