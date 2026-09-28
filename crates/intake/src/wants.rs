//! What the pipeline needs and cannot fetch where it runs. In a browser the
//! module has no network of its own: the picture fetchers note here what
//! they want, the page reads the list, fetches each item through the
//! manager and puts the file under the home, marks it missing, or hands
//! the answer in. On a machine the fetchers fetch for themselves and the
//! list stays empty.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// One thing the host is asked for.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Want {
    /// A file to fetch at `url` and put under `path` in the home, or to
    /// mark missing when the manager has none.
    File { path: String, url: String },
    /// The artist's fanart set and the slideshow settings, answered with
    /// [`answer_fanart`].
    Fanart { artist: String, uri: String },
}

static HOST: AtomicBool = AtomicBool::new(false);

/// Pictures come from the host from now on: the album art, the fanart and
/// the pictures of the track's folder are wanted here rather than fetched,
/// and the track's folder is read through the manager at the host's own
/// origin. Set once by a browser module; never on a machine.
pub fn set_host_pictures() {
    HOST.store(true, Ordering::Relaxed);
    super::set_manager("");
}

/// Whether the host brings the pictures.
pub fn host_pictures() -> bool {
    HOST.load(Ordering::Relaxed)
}

fn list() -> &'static Mutex<Vec<Want>> {
    static LIST: OnceLock<Mutex<Vec<Want>>> = OnceLock::new();
    LIST.get_or_init(|| Mutex::new(Vec::new()))
}

fn answer() -> &'static Mutex<Option<String>> {
    static ANSWER: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ANSWER.get_or_init(|| Mutex::new(None))
}

fn add(want: Want) {
    if let Ok(mut list) = list().lock() {
        if !list.contains(&want) {
            list.push(want);
        }
    }
}

/// A file wanted at `url`, to be put under `path` in the home.
pub fn file(path: &str, url: &str) {
    add(Want::File {
        path: path.to_string(),
        url: url.to_string(),
    });
}

/// The fanart set for an artist and the track's location.
pub fn fanart(artist: &str, uri: &str) {
    add(Want::Fanart {
        artist: artist.to_string(),
        uri: uri.to_string(),
    });
}

/// Everything wanted since the last take. A want not satisfied is asked
/// for again at the next chance, so a host may lose one.
pub fn take() -> Vec<Want> {
    list()
        .lock()
        .map(|mut list| std::mem::take(&mut *list))
        .unwrap_or_default()
}

/// The host's answer to a fanart want, as the manager's JSON.
pub fn answer_fanart(text: &str) {
    if let Ok(mut slot) = answer().lock() {
        *slot = Some(text.to_string());
    }
}

/// The fanart answer handed in, once.
pub fn take_fanart_answer() -> Option<String> {
    answer().lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_want_is_listed_once_and_taken_once() {
        let _ = take();
        file("art/a.img", "/api/face/picture?at=a");
        file("art/a.img", "/api/face/picture?at=a");
        fanart("Someone", "mnt/x/y.flac");
        let taken = take();
        assert_eq!(
            taken,
            vec![
                Want::File {
                    path: "art/a.img".into(),
                    url: "/api/face/picture?at=a".into()
                },
                Want::Fanart {
                    artist: "Someone".into(),
                    uri: "mnt/x/y.flac".into()
                }
            ]
        );
        assert!(take().is_empty());
        assert_eq!(
            serde_json::to_string(&taken[1]).unwrap(),
            r#"{"kind":"fanart","artist":"Someone","uri":"mnt/x/y.flac"}"#
        );
        answer_fanart("{}");
        assert_eq!(take_fanart_answer().as_deref(), Some("{}"));
        assert!(take_fanart_answer().is_none());
    }
}
