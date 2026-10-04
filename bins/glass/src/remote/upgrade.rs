//! A remote display brings itself up to date. It asks GitHub for the latest
//! release of what it is, the standalone remote or a display built with a
//! face, fetches the archive for this machine, checks it against the size
//! and the digest the release states, tries the new binary, puts it in the
//! running one's place with the one before kept beside it, and starts
//! again as the new one.
//!
//! What may be fetched is fixed here and by the face: the repository, the
//! archive's name, the file inside it. The page that asks for an upgrade
//! names none of them, so all a request can do is bring the latest release
//! that is no pre-release.
//!
//! A new binary that cannot hold on does not stay: it is tried with
//! `--version` before it takes the place, and a note of the upgrade is kept
//! until a start has lived a minute; a third start that finds the note puts
//! the binary before back.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// GitHub's API, where the releases are asked. `GLASS_RELEASES_API` names
/// another address for a trial against a stand-in; unset in use.
const API: &str = "https://api.github.com";
/// The most an archive may weigh, packed and unpacked.
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_UNPACKED: u64 = 256 * 1024 * 1024;
/// How long a new binary has to say its version.
const TRY_WITHIN: Duration = Duration::from_secs(10);
/// How long a start has to live before an upgrade counts as taken, and the
/// start at which a binary that never did is put back.
pub const SETTLED_AFTER: Duration = Duration::from_secs(60);
const STARTS_ALLOWED: u32 = 2;
const NOTE: &str = "upgrade.json";

/// What this display is a release of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Product {
    /// As the page says it: `Glass`, `glass-evo`.
    pub name: String,
    pub repository: String,
    /// What its archives are called before the version.
    pub asset: String,
    /// The binary's name inside an archive.
    pub binary: String,
    pub version: String,
}

/// The product a display is: Glass's own where it carries no face, the
/// face's where the face says where it is released, none where a face
/// says nothing (the display's own release would take the face away).
pub fn product(has_face: bool, origin: Option<overlay::Origin>) -> Option<Product> {
    match (has_face, origin) {
        (false, _) => Some(Product {
            name: "Glass".to_string(),
            repository: "foonerd/glass".to_string(),
            asset: "glass-".to_string(),
            binary: "glass".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
        (true, Some(origin)) => Some(Product {
            name: origin.asset.trim_end_matches('-').to_string(),
            repository: origin.repository,
            asset: origin.asset,
            binary: origin.binary,
            version: origin.version,
        }),
        (true, None) => None,
    }
}

/// The folders an archive keeps this machine's binary under, the first
/// that is there taken; none on a machine no upgrade is offered for yet.
pub fn arch_folders() -> &'static [&'static str] {
    if !cfg!(target_os = "linux") {
        return &[];
    }
    match std::env::consts::ARCH {
        "x86_64" => &["x64"],
        "aarch64" => &["armv8"],
        "arm" => &["armv7", "arm"],
        _ => &[],
    }
}

/// The archive's name for this machine at a version.
pub fn archive_name(asset: &str, version: &str, folders: &[&str]) -> Option<String> {
    folders
        .first()
        .map(|arch| format!("{asset}{version}-{arch}.tar.gz"))
}

/// A release as far as an upgrade needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    pub page: String,
}

/// Three numbers of a version, a `v` before them passed over; none for
/// anything else.
fn numbers(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// Whether `latest` is a later version than `current`.
pub fn newer(latest: &str, current: &str) -> bool {
    match (numbers(latest), numbers(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The release in GitHub's answer, with this machine's archive: its
/// address, its size and its digest as the release states them. `strict`
/// holds the address to the product's own releases on GitHub.
pub fn release_of(
    body: &Value,
    product: &Product,
    folders: &[&str],
    strict: bool,
) -> Result<Release, String> {
    let tag = body.get("tag_name").and_then(Value::as_str).unwrap_or("");
    let version = tag.trim_start_matches('v').to_string();
    if numbers(&version).is_none() {
        return Err(format!("the release's tag is no version: {tag:?}"));
    }
    if body.get("prerelease").and_then(Value::as_bool) == Some(true)
        || body.get("draft").and_then(Value::as_bool) == Some(true)
    {
        return Err("the release is a test release".to_string());
    }
    let wanted = archive_name(&product.asset, &version, folders)
        .ok_or("no upgrade is offered for this machine yet")?;
    let asset = body
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find(|a| a.get("name").and_then(Value::as_str) == Some(wanted.as_str()))
        })
        .ok_or_else(|| format!("the release {version} holds no {wanted}"))?;
    let url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let home = format!(
        "https://github.com/{}/releases/download/",
        product.repository
    );
    if strict && !url.starts_with(&home) {
        return Err(format!(
            "the archive's address is not the release's own: {url}"
        ));
    }
    let bytes = asset.get("size").and_then(Value::as_u64).unwrap_or(0);
    if bytes == 0 || bytes > MAX_ARCHIVE {
        return Err(format!("the archive's size is not usable: {bytes}"));
    }
    let sha256 = asset
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("the release states no checksum for the archive")?
        .to_ascii_lowercase();
    Ok(Release {
        version,
        url,
        bytes,
        sha256,
        page: body
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// Where the releases are asked, and whether that is GitHub itself.
fn api() -> (String, bool) {
    match std::env::var("GLASS_RELEASES_API") {
        Ok(other) if !other.trim().is_empty() => {
            (other.trim().trim_end_matches('/').to_string(), false)
        }
        _ => (API.to_string(), true),
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent()
}

/// The latest release of the product that is no test release, with this
/// machine's archive.
pub fn latest(product: &Product) -> Result<Release, String> {
    let (base, strict) = api();
    let url = format!("{base}/repos/{}/releases/latest", product.repository);
    let text = agent(Duration::from_secs(20))
        .get(&url)
        .header(
            "User-Agent",
            concat!("glass-remote/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("the releases could not be read: {e}"))?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("the releases could not be read: {e}"))?;
    let body: Value = serde_json::from_str(&text)
        .map_err(|e| format!("the releases' answer is not JSON: {e}"))?;
    release_of(&body, product, arch_folders(), strict)
}

/// The release's archive, whole and checked: its size and its digest as
/// the release states them. `told` hears how many bytes have come.
pub fn download(release: &Release, mut told: impl FnMut(u64)) -> Result<Vec<u8>, String> {
    let mut response = agent(Duration::from_secs(900))
        .get(&release.url)
        .header(
            "User-Agent",
            concat!("glass-remote/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("the archive could not be fetched: {e}"))?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(release.bytes + 1)
        .reader();
    let mut archive = Vec::with_capacity(release.bytes as usize);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|e| format!("the download broke: {e}"))?;
        if n == 0 {
            break;
        }
        archive.extend_from_slice(&chunk[..n]);
        told(archive.len() as u64);
    }
    checked(release, archive)
}

/// An archive as fetched, if it is the one the release states.
pub fn checked(release: &Release, archive: Vec<u8>) -> Result<Vec<u8>, String> {
    if archive.len() as u64 != release.bytes {
        return Err(format!(
            "the archive has {} bytes, the release says {}",
            archive.len(),
            release.bytes
        ));
    }
    let digest = Sha256::digest(&archive)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if digest != release.sha256 {
        return Err("the archive does not match the release's checksum".to_string());
    }
    Ok(archive)
}

/// A text field of a tar header: up to the first NUL.
fn field(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// The first regular file of a gzipped tar whose path ends in one of
/// `ends`, the earlier of them before the later.
pub fn file_from_tar_gz(archive: &[u8], ends: &[String]) -> Result<Vec<u8>, String> {
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(archive)
        .take(MAX_UNPACKED + 1)
        .read_to_end(&mut tar)
        .map_err(|e| format!("the archive could not be unpacked: {e}"))?;
    if tar.len() as u64 > MAX_UNPACKED {
        return Err("the archive unpacks to more than allowed".to_string());
    }
    let mut found: Vec<Option<(usize, usize)>> = vec![None; ends.len()];
    let mut at = 0usize;
    let mut long_name: Option<String> = None;
    while at + 512 <= tar.len() {
        let header = &tar[at..at + 512];
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let size = usize::from_str_radix(field(&header[124..136]).trim(), 8)
            .map_err(|_| "the archive's table is not readable".to_string())?;
        let data = at + 512;
        let next = data + size.div_ceil(512) * 512;
        if data + size > tar.len() {
            return Err("the archive is cut short".to_string());
        }
        let kind = header[156];
        if kind == b'L' {
            // A long name for the entry that follows, as GNU tar writes it.
            long_name = Some(field(&tar[data..data + size]));
        } else {
            let name = long_name.take().unwrap_or_else(|| {
                let name = field(&header[0..100]);
                let prefix = if &header[257..262] == b"ustar" {
                    field(&header[345..500])
                } else {
                    String::new()
                };
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            });
            if kind == b'0' || kind == 0 {
                for (slot, end) in found.iter_mut().zip(ends) {
                    if slot.is_none() && (name == *end || name.ends_with(&format!("/{end}"))) {
                        *slot = Some((data, size));
                    }
                }
            }
        }
        at = next;
    }
    found
        .into_iter()
        .flatten()
        .next()
        .map(|(data, size)| tar[data..data + size].to_vec())
        .ok_or_else(|| format!("the archive holds no {}", ends.join(" or ")))
}

/// The paths inside an archive this machine's binary may lie at.
pub fn binary_paths(binary: &str, folders: &[&str]) -> Vec<String> {
    folders
        .iter()
        .map(|arch| format!("bin/{arch}/{binary}"))
        .collect()
}

fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

/// What a binary answers to `--version`, its first line; an error where it
/// does not start, does not answer in time or ends in failure. Started
/// through `posix_spawn` directly, its answer taken from a file: the
/// standard library's spawn refers to glibc 2.39's `pidfd_spawnp`, and a
/// binary linked against a 2.39 sysroot then refuses to load on Volumio's
/// glibc 2.36.
#[cfg(all(unix, not(target_os = "android")))]
fn version_said(binary: &Path) -> Result<String, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    extern "C" {
        static environ: *const *mut libc::c_char;
    }
    let c = |bytes: &[u8]| CString::new(bytes).map_err(|e| e.to_string());
    let program = c(binary.as_os_str().as_bytes())?;
    let said_file = beside(binary, ".said");
    let said_c = c(said_file.as_os_str().as_bytes())?;
    let null = c(b"/dev/null")?;
    let arg = c(b"--version")?;
    let argv = [
        program.as_ptr() as *mut libc::c_char,
        arg.as_ptr() as *mut libc::c_char,
        std::ptr::null_mut(),
    ];
    let mut pid: libc::pid_t = 0;
    // SAFETY: every pointer handed over lives until the call returns, the
    // argument array ends with a null, and posix_spawn reads and does not
    // keep them; the file actions are made and destroyed here.
    let rc = unsafe {
        let mut actions: libc::posix_spawn_file_actions_t = std::mem::zeroed();
        libc::posix_spawn_file_actions_init(&mut actions);
        libc::posix_spawn_file_actions_addopen(&mut actions, 0, null.as_ptr(), libc::O_RDONLY, 0);
        libc::posix_spawn_file_actions_addopen(
            &mut actions,
            1,
            said_c.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o600,
        );
        libc::posix_spawn_file_actions_addopen(&mut actions, 2, null.as_ptr(), libc::O_WRONLY, 0);
        let rc = libc::posix_spawn(
            &mut pid,
            program.as_ptr(),
            &actions,
            std::ptr::null(),
            argv.as_ptr(),
            environ,
        );
        libc::posix_spawn_file_actions_destroy(&mut actions);
        rc
    };
    if rc != 0 {
        let _ = std::fs::remove_file(&said_file);
        return Err(format!(
            "the new binary does not start: {}",
            std::io::Error::from_raw_os_error(rc)
        ));
    }
    let started = Instant::now();
    let mut status = 0;
    let ended = loop {
        // SAFETY: waiting on the child started above.
        let done = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if done == pid {
            break Ok(());
        }
        if done < 0 {
            break Err("the new binary could not be waited for".to_string());
        }
        if started.elapsed() >= TRY_WITHIN {
            // SAFETY: ending and reaping the child started above.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
            break Err("the new binary did not answer in time".to_string());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let said = std::fs::read_to_string(&said_file).unwrap_or_default();
    let _ = std::fs::remove_file(&said_file);
    ended?;
    if !(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0) {
        return Err("the new binary ended in failure".to_string());
    }
    Ok(said.lines().next().unwrap_or("").trim().to_string())
}

#[cfg(any(not(unix), target_os = "android"))]
fn version_said(binary: &Path) -> Result<String, String> {
    Err(format!(
        "{}: not tried on this system yet",
        binary.display()
    ))
}

/// The note kept from an upgrade until a start has lived a minute.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Note {
    from: String,
    to: String,
    starts: u32,
}

/// A new binary in the running one's place: written beside it, tried (it
/// must start here and say the release's version), then swapped in with
/// the one before kept as `<name>.prev`. A note of it goes into the cache
/// folder for the starts that follow.
pub fn install(
    binary: &[u8],
    exe: &Path,
    cache: &Path,
    product: &Product,
    release: &Release,
) -> Result<(), String> {
    let new = beside(exe, ".new");
    let prev = beside(exe, ".prev");
    std::fs::write(&new, binary).map_err(|e| format!("{}: {e}", new.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", new.display()))?;
    }
    let tried = version_said(&new).and_then(|said| {
        if said.split_whitespace().last() == Some(release.version.as_str()) {
            Ok(())
        } else {
            Err(format!(
                "the new binary says {said:?}, the release is {}",
                release.version
            ))
        }
    });
    if let Err(why) = tried {
        let _ = std::fs::remove_file(&new);
        return Err(why);
    }
    let _ = std::fs::remove_file(&prev);
    std::fs::rename(exe, &prev).map_err(|e| format!("{}: {e}", exe.display()))?;
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&prev, exe);
        let _ = std::fs::remove_file(&new);
        return Err(format!("{}: {e}", exe.display()));
    }
    let note = Note {
        from: product.version.clone(),
        to: release.version.clone(),
        starts: 0,
    };
    let _ = std::fs::create_dir_all(cache);
    let _ = std::fs::write(
        cache.join(NOTE),
        serde_json::to_string(&note).unwrap_or_default(),
    );
    Ok(())
}

/// What a start finds of an upgrade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtStart {
    /// No upgrade is on trial.
    Nothing,
    /// The binary of an upgrade starts, for the nth time since.
    Trying(u32),
    /// It never lived a minute: the binary before is back in place, and
    /// this process is to leave for it. From and to, for the log.
    PutBack(String, String),
}

/// Asked once at every start, before the window: counts the start against
/// an upgrade on trial, and puts the binary before back when the new one
/// has started more often than allowed without living a minute.
pub fn at_start(cache: &Path, exe: &Path) -> AtStart {
    let file = cache.join(NOTE);
    let Some(mut note) = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<Note>(&text).ok())
    else {
        return AtStart::Nothing;
    };
    note.starts += 1;
    if note.starts <= STARTS_ALLOWED {
        let _ = std::fs::write(&file, serde_json::to_string(&note).unwrap_or_default());
        return AtStart::Trying(note.starts);
    }
    let _ = std::fs::remove_file(&file);
    let prev = beside(exe, ".prev");
    if !prev.is_file() {
        return AtStart::Nothing;
    }
    let failed = beside(exe, ".failed");
    let _ = std::fs::remove_file(&failed);
    if std::fs::rename(exe, &failed).is_err() {
        return AtStart::Nothing;
    }
    if std::fs::rename(&prev, exe).is_err() {
        let _ = std::fs::rename(&failed, exe);
        return AtStart::Nothing;
    }
    AtStart::PutBack(note.to, note.from)
}

/// A start has lived its minute: the upgrade is taken.
pub fn settled(cache: &Path) {
    let _ = std::fs::remove_file(cache.join(NOTE));
}

/// How the display becomes the binary now in its place: under the user
/// service it leaves and is started again; started any other way it
/// replaces itself.
pub fn under_service() -> bool {
    std::env::var_os("INVOCATION_ID").is_some()
}

/// Replace this process with the binary now in its place, with the
/// arguments this one was started with. Returns only where that failed.
/// Through `execv` directly, for the reason `version_said` gives.
#[cfg(all(unix, not(target_os = "android")))]
pub fn become_new(exe: &Path) -> String {
    use std::ffi::CString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let Ok(program) = CString::new(exe.as_os_str().as_bytes()) else {
        return format!("{}: not a path a program can have", exe.display());
    };
    let args: Vec<CString> = std::iter::once(program.clone())
        .chain(
            std::env::args_os()
                .skip(1)
                .filter_map(|a| CString::new(a.into_vec()).ok()),
        )
        .collect();
    let mut argv: Vec<*const libc::c_char> = args.iter().map(|a| a.as_ptr()).collect();
    argv.push(std::ptr::null());
    // SAFETY: the program and every argument live until the call, the
    // array ends with a null, and execv returns only where it failed.
    unsafe { libc::execv(program.as_ptr(), argv.as_ptr()) };
    format!("{}: {}", exe.display(), std::io::Error::last_os_error())
}

#[cfg(any(not(unix), target_os = "android"))]
pub fn become_new(exe: &Path) -> String {
    format!("{}: not on this system yet", exe.display())
}

/// Where an upgrade stands, for the page.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct State {
    /// `idle`, `checking`, `downloading`, `installing`, `restarting`.
    pub phase: String,
    pub latest: Option<Release>,
    /// Whether `latest` is later than what runs.
    pub available: bool,
    /// Seconds since 1970 of the last look at the releases.
    pub checked_at: u64,
    pub done: u64,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn glass() -> Product {
        product(false, None).expect("the standalone is Glass")
    }

    fn evo() -> Product {
        product(
            true,
            Some(overlay::Origin {
                repository: "foonerd/glass-evo".to_string(),
                asset: "glass-evo-".to_string(),
                binary: "glass-evo".to_string(),
                version: "0.1.27".to_string(),
            }),
        )
        .expect("a face that says where it is released")
    }

    fn answer(tag: &str, name: &str, url: &str, size: u64, digest: &str) -> Value {
        serde_json::json!({
            "tag_name": tag, "html_url": "https://github.com/foonerd/glass/releases/tag/x",
            "assets": [
                { "name": "glass-0.9.0.zip", "size": 9, "digest": "sha256:00", "browser_download_url": "zip" },
                { "name": name, "size": size, "digest": digest, "browser_download_url": url }
            ]
        })
    }

    /// A tar of regular files, as `tar` writes them, gzipped.
    fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for (name, data) in files {
            let mut header = [0u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            header[100..107].copy_from_slice(b"0000755");
            let size = format!("{:011o}", data.len());
            header[124..135].copy_from_slice(size.as_bytes());
            header[156] = b'0';
            header[257..262].copy_from_slice(b"ustar");
            tar.extend_from_slice(&header);
            tar.extend_from_slice(data);
            tar.resize(tar.len().div_ceil(512) * 512, 0);
        }
        tar.extend_from_slice(&[0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn a_display_is_glass_or_its_faces_product_or_none() {
        let g = glass();
        assert_eq!(
            (
                g.name.as_str(),
                g.repository.as_str(),
                g.asset.as_str(),
                g.binary.as_str()
            ),
            ("Glass", "foonerd/glass", "glass-", "glass")
        );
        assert_eq!(g.version, env!("CARGO_PKG_VERSION"));
        let e = evo();
        assert_eq!(
            (e.name.as_str(), e.version.as_str()),
            ("glass-evo", "0.1.27")
        );
        assert_eq!(
            product(true, None),
            None,
            "a face that says nothing is offered nothing"
        );
    }

    #[test]
    fn a_version_is_later_by_its_numbers() {
        assert!(newer("0.8.53", "0.8.52"));
        assert!(newer("v0.9.0", "0.8.99"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("0.8.52", "0.8.52"));
        assert!(!newer("0.8.51", "0.8.52"));
        assert!(!newer("nonsense", "0.8.52"));
        assert!(
            !newer("0.8.53-rc1", "0.8.52"),
            "only a plain version is one"
        );
    }

    #[test]
    fn the_release_names_this_machines_archive_with_its_size_and_digest() {
        let digest = format!("sha256:{}", "ab".repeat(32));
        let url =
            "https://github.com/foonerd/glass/releases/download/v0.9.0/glass-0.9.0-x64.tar.gz";
        let body = answer("v0.9.0", "glass-0.9.0-x64.tar.gz", url, 4_500_000, &digest);
        let release = release_of(&body, &glass(), &["x64"], true).expect("a release");
        assert_eq!(
            (
                release.version.as_str(),
                release.url.as_str(),
                release.bytes
            ),
            ("0.9.0", url, 4_500_000)
        );
        assert_eq!(release.sha256, "ab".repeat(32));
        // armv7 is asked for by its own name; a machine with no build is offered nothing.
        assert!(release_of(&body, &glass(), &["armv7", "arm"], true)
            .unwrap_err()
            .contains("glass-0.9.0-armv7.tar.gz"));
        assert!(release_of(&body, &glass(), &[], true)
            .unwrap_err()
            .contains("no upgrade is offered"));
        // An address that is not the release's own is refused, but for a stand-in.
        let elsewhere = answer(
            "v0.9.0",
            "glass-0.9.0-x64.tar.gz",
            "https://example.org/x.tar.gz",
            5,
            &digest,
        );
        assert!(release_of(&elsewhere, &glass(), &["x64"], true).is_err());
        assert!(release_of(&elsewhere, &glass(), &["x64"], false).is_ok());
        // Another product's release is not this one's.
        assert!(release_of(&body, &evo(), &["x64"], true).is_err());
        // No digest, no size, a test release, a tag that is no version: none.
        for bad in [
            answer("v0.9.0", "glass-0.9.0-x64.tar.gz", url, 5, ""),
            answer("v0.9.0", "glass-0.9.0-x64.tar.gz", url, 5, "sha256:xyz"),
            answer("v0.9.0", "glass-0.9.0-x64.tar.gz", url, 0, &digest),
            answer("latest", "glass-0.9.0-x64.tar.gz", url, 5, &digest),
        ] {
            assert!(release_of(&bad, &glass(), &["x64"], true).is_err(), "{bad}");
        }
        let mut test_release = body.clone();
        test_release["prerelease"] = Value::Bool(true);
        assert!(release_of(&test_release, &glass(), &["x64"], true).is_err());
    }

    #[test]
    fn an_archive_is_taken_only_as_the_release_states_it() {
        let archive = b"an archive".to_vec();
        let digest = Sha256::digest(&archive)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let release = Release {
            version: "0.9.0".into(),
            url: String::new(),
            bytes: archive.len() as u64,
            sha256: digest,
            page: String::new(),
        };
        assert_eq!(checked(&release, archive.clone()), Ok(archive.clone()));
        assert!(checked(&release, b"an archivE".to_vec())
            .unwrap_err()
            .contains("checksum"));
        assert!(checked(&release, b"short".to_vec())
            .unwrap_err()
            .contains("bytes"));
    }

    #[test]
    fn the_binary_is_found_in_the_archive_by_its_folder() {
        let archive = tar_gz(&[
            ("glass-0.9.0-armv7/remote/linux/install.sh", b"#!/bin/sh\n"),
            ("glass-0.9.0-armv7/bin/arm/glass", b"the arm binary"),
            ("glass-0.9.0-armv7/bin/arm/glass-serve", b"another program"),
        ]);
        let paths = binary_paths("glass", &["armv7", "arm"]);
        assert_eq!(paths, vec!["bin/armv7/glass", "bin/arm/glass"]);
        assert_eq!(
            file_from_tar_gz(&archive, &paths).unwrap(),
            b"the arm binary"
        );
        // The first folder named wins where both are there.
        let both = tar_gz(&[
            ("a/bin/arm/glass", b"older name"),
            ("a/bin/armv7/glass", b"its own name"),
        ]);
        assert_eq!(file_from_tar_gz(&both, &paths).unwrap(), b"its own name");
        assert!(
            file_from_tar_gz(&archive, &binary_paths("glass-evo", &["armv7", "arm"]))
                .unwrap_err()
                .contains("holds no")
        );
        assert!(file_from_tar_gz(b"not gzip at all", &paths).is_err());
        // A file larger than a block, and one cut short.
        let big = vec![7u8; 1500];
        let archive = tar_gz(&[("x/bin/x64/glass", &big)]);
        assert_eq!(
            file_from_tar_gz(&archive, &binary_paths("glass", &["x64"])).unwrap(),
            big
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_new_binary_takes_the_place_only_if_it_runs_and_says_the_version() {
        let dir = std::env::temp_dir().join(format!("glass-upgrade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let cache = dir.join("cache");
        let exe = dir.join("bin/glass");
        std::fs::write(&exe, "#!/bin/sh\necho glass 0.8.0\n").unwrap();
        let release = |version: &str| Release {
            version: version.into(),
            url: String::new(),
            bytes: 1,
            sha256: String::new(),
            page: String::new(),
        };
        let product = glass();
        // One that says another version, one that fails, one that is no program: the old one stays.
        for bad in [
            "#!/bin/sh\necho glass 0.8.9\n",
            "#!/bin/sh\nexit 3\n",
            "not a program",
        ] {
            assert!(install(bad.as_bytes(), &exe, &cache, &product, &release("0.9.0")).is_err());
            assert_eq!(
                std::fs::read_to_string(&exe).unwrap(),
                "#!/bin/sh\necho glass 0.8.0\n"
            );
            assert!(!dir.join("bin/glass.new").exists() && !dir.join("bin/glass.prev").exists());
            assert_eq!(at_start(&cache, &exe), AtStart::Nothing);
        }
        // One that answers: in place, the one before kept, a note written.
        let good = "#!/bin/sh\necho glass 0.9.0\n";
        install(good.as_bytes(), &exe, &cache, &product, &release("0.9.0")).expect("installed");
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), good);
        assert_eq!(
            std::fs::read_to_string(dir.join("bin/glass.prev")).unwrap(),
            "#!/bin/sh\necho glass 0.8.0\n"
        );
        // Two starts are on trial; one that lives its minute settles it.
        assert_eq!(at_start(&cache, &exe), AtStart::Trying(1));
        assert_eq!(at_start(&cache, &exe), AtStart::Trying(2));
        settled(&cache);
        assert_eq!(at_start(&cache, &exe), AtStart::Nothing);
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), good);
        // Again, and this time no start lives: the third puts the one before back.
        std::fs::write(&exe, "#!/bin/sh\necho glass 0.8.0\n").unwrap();
        install(good.as_bytes(), &exe, &cache, &product, &release("0.9.0")).expect("installed");
        assert_eq!(at_start(&cache, &exe), AtStart::Trying(1));
        assert_eq!(at_start(&cache, &exe), AtStart::Trying(2));
        assert_eq!(
            at_start(&cache, &exe),
            AtStart::PutBack("0.9.0".into(), env!("CARGO_PKG_VERSION").into())
        );
        assert_eq!(
            std::fs::read_to_string(&exe).unwrap(),
            "#!/bin/sh\necho glass 0.8.0\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("bin/glass.failed")).unwrap(),
            good
        );
        assert_eq!(
            at_start(&cache, &exe),
            AtStart::Nothing,
            "and the note is gone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
