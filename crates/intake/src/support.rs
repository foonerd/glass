//! What a remote can say of the theme's assets, for its page and its
//! support bundle: the files the theme names, each offered by the player
//! or not, brought here or not, with the reason where it failed. Pure over
//! what the sync recorded and what is on disk, so a test can hold it.

use serde::Serialize;

/// One file the theme on show names.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RequiredFile {
    /// The key that names it, `bgr.filename`, `time.total.font`, …
    pub key: String,
    /// As the theme wrote it.
    pub file: String,
    /// `brought` (here, as the player offered it), `failed` (offered, and
    /// the fetch failed: the reason says why), `missing` (the theme names
    /// it and the player does not have it: the player's own screen lacks it
    /// too), `elsewhere` (a path on the player, not a theme file: the
    /// display looks for it where it runs).
    pub state: String,
    pub reason: String,
}

/// The theme's assets as the remote stands: the files named and their
/// states, and the counts the page shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AssetsReport {
    pub theme: String,
    pub required: Vec<RequiredFile>,
    pub brought: usize,
    pub failed: usize,
    pub missing: usize,
    /// Files the player offered that the fetch failed, the theme's and the
    /// rest (fonts, icons, the look's), each with the reason.
    pub fetch_failed: Vec<(String, String)>,
    /// How many files the player offered in all, and how many of them are here.
    pub offered: usize,
    pub here: usize,
}

/// The report for `theme`, from its text, what the player offered (paths
/// under the home), what failed (path, reason), and whether a path under
/// the home is a file now.
pub fn assets_report(
    theme: &str,
    meters_text: &str,
    offered: &[String],
    fetch_failed: &[(String, String)],
    exists: impl Fn(&str) -> bool,
) -> AssetsReport {
    let folder = format!("templates/{theme}/");
    let mut required = Vec::new();
    for (key, file) in lead::tailor::files_named(meters_text) {
        let state;
        let mut reason = String::new();
        if file.starts_with('/') || file.contains(":\\") {
            state = "elsewhere";
        } else {
            let relative = format!("{folder}{}", file.trim_start_matches("./"));
            if let Some((_, why)) = fetch_failed.iter().find(|(p, _)| p == &relative) {
                state = "failed";
                reason = why.clone();
            } else if exists(&relative) {
                state = "brought";
            } else if offered.iter().any(|p| p == &relative) {
                state = "failed";
                reason = "offered by the player and not here".to_string();
            } else {
                state = "missing";
            }
        }
        required.push(RequiredFile {
            key,
            file,
            state: state.to_string(),
            reason,
        });
    }
    let count = |s: &str| required.iter().filter(|r| r.state == s).count();
    AssetsReport {
        theme: theme.to_string(),
        brought: count("brought"),
        failed: count("failed"),
        missing: count("missing"),
        required,
        fetch_failed: fetch_failed.to_vec(),
        offered: offered.len(),
        here: offered.iter().filter(|p| exists(p)).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: &str = "[gold]\nmeter.type = circular\nbgr.filename = gold-bgr.png\nfgr.filename = gold-fgr.png\nbutton.play.image = play.png, play-lit.png\ntime.total.font = fonts/Another.ttf\nplayinfo.title.font = /usr/share/fonts/Other.ttf\nvinyl.filename = none\n";

    #[test]
    fn each_file_the_theme_names_is_brought_failed_missing_or_elsewhere() {
        let offered = vec![
            "templates/t/meters.txt".to_string(),
            "templates/t/gold-bgr.png".to_string(),
            "templates/t/gold-fgr.png".to_string(),
            "templates/t/play.png".to_string(),
            "fonts/Light.ttf".to_string(),
        ];
        let failed = vec![(
            "templates/t/gold-fgr.png".to_string(),
            "HTTP 404".to_string(),
        )];
        let here = [
            "templates/t/meters.txt",
            "templates/t/gold-bgr.png",
            "fonts/Light.ttf",
        ];
        let report = assets_report("t", THEME, &offered, &failed, |p| here.contains(&p));
        let states: Vec<(&str, &str, &str)> = report
            .required
            .iter()
            .map(|r| (r.key.as_str(), r.file.as_str(), r.state.as_str()))
            .collect();
        assert_eq!(
            states,
            vec![
                ("bgr.filename", "gold-bgr.png", "brought"),
                ("fgr.filename", "gold-fgr.png", "failed"),
                ("button.play.image", "play.png", "failed"),
                ("button.play.image", "play-lit.png", "missing"),
                ("time.total.font", "fonts/Another.ttf", "missing"),
                (
                    "playinfo.title.font",
                    "/usr/share/fonts/Other.ttf",
                    "elsewhere"
                ),
            ]
        );
        assert_eq!(report.required[1].reason, "HTTP 404");
        assert_eq!(
            report.required[2].reason,
            "offered by the player and not here"
        );
        assert_eq!((report.brought, report.failed, report.missing), (1, 2, 2));
        assert_eq!((report.offered, report.here), (5, 3));
        assert_eq!(report.theme, "t");
    }

    #[test]
    fn a_theme_that_names_nothing_has_an_empty_report() {
        let report = assets_report("t", "[gold]\nmeter.type = circular\n", &[], &[], |_| false);
        assert!(report.required.is_empty());
        assert_eq!((report.brought, report.failed, report.missing), (0, 0, 0));
    }
}
