//! A remote display brings itself up to date. It asks GitHub for the latest
//! release of what it is, the standalone remote or a display built with a
//! face, fetches the archive for this machine, checks it against the size
//! and the digest the release states, tries the new binary, puts it in the
//! running one's place with the one before kept beside it, and starts
//! again as the new one.
//!
//! What may be fetched is fixed here and by the face: the repository, the
//! archive's name, the file inside it. The page that asks for an upgrade
//! names none of them, so all a request can do is bring the release
//! offered: the latest, which is never a pre-release, or on a remote set
//! to take test releases the newest of the repository's last ten, as the
//! player's own Manager offers them.
//!
//! A new binary that cannot hold on does not stay: it is tried with
//! `--version` before it takes the place, and a note of the upgrade is kept
//! until a start has lived a minute; a third start that finds the note puts
//! the binary before back.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
// Only where a new binary is tried before it takes the place: not on Android,
// whose app the system installs.
#[cfg(any(windows, all(unix, not(target_os = "android"))))]
use std::time::Instant;

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
#[cfg(any(windows, all(unix, not(target_os = "android"))))]
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
/// says nothing (the display's own release would take the face away), and
/// none on a machine no upgrade is offered for yet.
pub fn product(has_face: bool, origin: Option<overlay::Origin>) -> Option<Product> {
    if arch_folders().is_empty() {
        return None;
    }
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

/// What this machine's archive is called after the version, and with it
/// the folder its binary lies under: the first that is there taken. None on
/// a machine no upgrade is offered for yet. A Windows archive is a zip with
/// the display at `bin/<name>.exe`; the others are tars with it at
/// `bin/<arch>/<name>`.
pub fn arch_folders() -> &'static [&'static str] {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return &["windows-x64"];
    }
    // One package for every Android device; the system installs it.
    if cfg!(target_os = "android") {
        return &["android"];
    }
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
    folders.first().map(|arch| {
        let packed = if arch.starts_with("windows") {
            "zip"
        } else if *arch == "android" {
            "apk"
        } else {
            "tar.gz"
        };
        format!("{asset}{version}-{arch}.{packed}")
    })
}

/// Whether this display can put a release in its own place. An Android app
/// cannot: its page links the release's package, and the system's installer
/// asks the user and checks the package's signature against the app's.
pub fn in_place() -> bool {
    !cfg!(target_os = "android")
}

/// A release as far as an upgrade needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
    pub page: String,
    /// A test release (GitHub's pre-release mark), offered only to a
    /// remote set to take them.
    #[serde(default)]
    pub test: bool,
    /// The archive's name in the release, as the signed sums name it.
    #[serde(default)]
    pub asset: String,
    /// The signed sums and their signature, where the release carries
    /// them; none for a release published before signing.
    #[serde(default)]
    pub sums_url: Option<String>,
    #[serde(default)]
    pub sig_url: Option<String>,
}

/// How many of the repository's last releases a remote on test releases
/// looks through, as the player's Manager does.
const LAST_RELEASES: u32 = 10;

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
/// holds the address to the product's own releases on GitHub. A test
/// release is refused: this is the latest, as a remote not set to take
/// test releases is offered it.
pub fn release_of(
    body: &Value,
    product: &Product,
    folders: &[&str],
    strict: bool,
) -> Result<Release, String> {
    release_from(body, product, folders, strict, false)
}

/// The newest of a list of releases, by version, that holds this machine's
/// archive: test releases among them, drafts and releases without the
/// archive passed over. What a remote set to take test releases is offered.
pub fn newest_of(
    list: &[Value],
    product: &Product,
    folders: &[&str],
    strict: bool,
) -> Result<Release, String> {
    let mut best: Option<Release> = None;
    for body in list {
        let Ok(release) = release_from(body, product, folders, strict, true) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|b| newer(&release.version, &b.version))
        {
            best = Some(release);
        }
    }
    best.ok_or_else(|| "no release of the last ten holds an archive for this machine".to_string())
}

/// One release read from GitHub's answer; `allow_test` takes a test
/// release as offered, marked so.
fn release_from(
    body: &Value,
    product: &Product,
    folders: &[&str],
    strict: bool,
    allow_test: bool,
) -> Result<Release, String> {
    let tag = body.get("tag_name").and_then(Value::as_str).unwrap_or("");
    let version = tag.trim_start_matches('v').to_string();
    if numbers(&version).is_none() {
        return Err(format!("the release's tag is no version: {tag:?}"));
    }
    if body.get("draft").and_then(Value::as_bool) == Some(true) {
        return Err("the release is a draft".to_string());
    }
    let test = body.get("prerelease").and_then(Value::as_bool) == Some(true);
    if test && !allow_test {
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
    // A signed release carries both the sums and the signature, small and
    // at the release's own address; one without either is unsigned.
    let small = |name: &str, max: u64| -> Option<String> {
        let a = body
            .get("assets")
            .and_then(Value::as_array)?
            .iter()
            .find(|a| a.get("name").and_then(Value::as_str) == Some(name))?;
        let size = a.get("size").and_then(Value::as_u64).unwrap_or(0);
        let url = a.get("browser_download_url").and_then(Value::as_str)?;
        (size > 0 && size <= max && (!strict || url.starts_with(&home))).then(|| url.to_string())
    };
    let (sums_url, sig_url) = match (
        small(super::signing::SUMS_NAME, super::signing::MAX_SUMS),
        small(super::signing::SIG_NAME, super::signing::MAX_SIG),
    ) {
        (Some(s), Some(g)) => (Some(s), Some(g)),
        _ => (None, None),
    };
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
        test,
        asset: wanted,
        sums_url,
        sig_url,
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

/// The release of the product this remote is offered, with this machine's
/// archive: the latest, which is never a test release, or with `test` the
/// newest of the repository's last ten whatever its mark.
pub fn offered(product: &Product, test: bool) -> Result<Release, String> {
    let (base, strict) = api();
    let url = if test {
        format!(
            "{base}/repos/{}/releases?per_page={LAST_RELEASES}",
            product.repository
        )
    } else {
        format!("{base}/repos/{}/releases/latest", product.repository)
    };
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
        .limit(4 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("the releases could not be read: {e}"))?;
    let body: Value = serde_json::from_str(&text)
        .map_err(|e| format!("the releases' answer is not JSON: {e}"))?;
    if test {
        let list = body
            .as_array()
            .ok_or("the releases' answer is not a list")?;
        newest_of(list, product, arch_folders(), strict)
    } else {
        release_of(&body, product, arch_folders(), strict)
    }
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
    let archive = checked(release, archive)?;
    // A signed release is held to its signature too; an unsigned one is
    // taken as before signing.
    let small = |url: &str, max: u64| -> Result<Vec<u8>, String> {
        let mut response = agent(Duration::from_secs(60))
            .get(url)
            .header(
                "User-Agent",
                concat!("glass-remote/", env!("CARGO_PKG_VERSION")),
            )
            .call()
            .map_err(|e| format!("the release's signature could not be fetched: {e}"))?;
        response
            .body_mut()
            .with_config()
            .limit(max)
            .read_to_vec()
            .map_err(|e| format!("the release's signature could not be read: {e}"))
    };
    let sums = release
        .sums_url
        .as_deref()
        .map(|u| small(u, super::signing::MAX_SUMS))
        .transpose()?;
    let sig = release
        .sig_url
        .as_deref()
        .map(|u| small(u, super::signing::MAX_SIG))
        .transpose()?;
    let key = super::signing::public_key()?;
    let held = super::signing::held_to_signature(
        &key,
        &release.asset,
        &release.sha256,
        sums.as_deref(),
        sig.as_deref(),
    )?;
    if held == "signed" {
        logline::say!(
            Info,
            "upgrade",
            "{} verified against the release's signature",
            release.asset
        );
    }
    Ok(archive)
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
        .map(|arch| {
            if arch.starts_with("windows") {
                format!("bin/{binary}.exe")
            } else {
                format!("bin/{arch}/{binary}")
            }
        })
        .collect()
}

fn le(bytes: &[u8], at: usize, len: usize) -> Option<usize> {
    let part = bytes.get(at..at + len)?;
    Some(
        part.iter()
            .rev()
            .fold(0usize, |n, b| (n << 8) | *b as usize),
    )
}

/// The first regular file of a zip whose path ends in one of `ends`, the
/// earlier of them before the later: stored or deflated, read from the
/// zip's own table at its end.
pub fn file_from_zip(archive: &[u8], ends: &[String]) -> Result<Vec<u8>, String> {
    let broken = || "the archive's table is not readable".to_string();
    // The end record: its signature, searched from the back past a comment.
    let end = (0..=archive.len().saturating_sub(22))
        .rev()
        .take(66_000)
        .find(|at| archive[*at..].starts_with(b"PK\x05\x06"))
        .ok_or_else(broken)?;
    let count = le(archive, end + 10, 2).ok_or_else(broken)?;
    let mut at = le(archive, end + 16, 4).ok_or_else(broken)?;
    let mut found: Vec<Option<(usize, usize, usize, usize)>> = vec![None; ends.len()];
    for _ in 0..count {
        if !archive
            .get(at..)
            .is_some_and(|rest| rest.starts_with(b"PK\x01\x02"))
        {
            return Err(broken());
        }
        let method = le(archive, at + 10, 2).ok_or_else(broken)?;
        let packed = le(archive, at + 20, 4).ok_or_else(broken)?;
        let size = le(archive, at + 24, 4).ok_or_else(broken)?;
        let name_len = le(archive, at + 28, 2).ok_or_else(broken)?;
        let extra_len = le(archive, at + 30, 2).ok_or_else(broken)?;
        let comment_len = le(archive, at + 32, 2).ok_or_else(broken)?;
        let local = le(archive, at + 42, 4).ok_or_else(broken)?;
        let name = archive
            .get(at + 46..at + 46 + name_len)
            .map(|n| String::from_utf8_lossy(n).replace('\\', "/"))
            .ok_or_else(broken)?;
        if !name.ends_with('/') {
            for (slot, end) in found.iter_mut().zip(ends) {
                if slot.is_none() && (name == *end || name.ends_with(&format!("/{end}"))) {
                    *slot = Some((method, packed, size, local));
                }
            }
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    let (method, packed, size, local) = found
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| format!("the archive holds no {}", ends.join(" or ")))?;
    if size as u64 > MAX_UNPACKED {
        return Err("the archive unpacks to more than allowed".to_string());
    }
    if !archive
        .get(local..)
        .is_some_and(|rest| rest.starts_with(b"PK\x03\x04"))
    {
        return Err(broken());
    }
    let data = local
        + 30
        + le(archive, local + 26, 2).ok_or_else(broken)?
        + le(archive, local + 28, 2).ok_or_else(broken)?;
    let bytes = archive
        .get(data..data + packed)
        .ok_or("the archive is cut short")?;
    match method {
        0 => Ok(bytes.to_vec()),
        8 => {
            let mut out = Vec::with_capacity(size);
            flate2::read::DeflateDecoder::new(bytes)
                .take(MAX_UNPACKED + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("the archive could not be unpacked: {e}"))?;
            if out.len() != size {
                return Err("the archive could not be unpacked: a file is not its size".to_string());
            }
            Ok(out)
        }
        other => Err(format!(
            "the archive packs a file in a way not read here ({other})"
        )),
    }
}

/// The display's binary out of a release's archive, a zip or a gzipped tar
/// by what it begins with.
pub fn binary_from(archive: &[u8], paths: &[String]) -> Result<Vec<u8>, String> {
    if archive.starts_with(b"PK") {
        file_from_zip(archive, paths)
    } else {
        file_from_tar_gz(archive, paths)
    }
}

fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let name = exe
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    // On Windows a program is one by its ending: glass.exe keeps it, as
    // glass.prev.exe.
    let named = match name.len().checked_sub(4) {
        Some(at) if name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case(".exe") => {
            format!("{}{suffix}{}", &name[..at], &name[at..])
        }
        _ => format!("{name}{suffix}"),
    };
    exe.with_file_name(named)
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

/// On Windows the standard library's own spawn serves: there is no glibc
/// to be newer than the player's. No console window opens for the try.
#[cfg(windows)]
fn version_said(binary: &Path) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(binary)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("the new binary does not start: {e}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut said = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut said);
                }
                if !status.success() {
                    return Err("the new binary ended in failure".to_string());
                }
                return Ok(said.lines().next().unwrap_or("").trim().to_string());
            }
            Ok(None) if started.elapsed() < TRY_WITHIN => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("the new binary did not answer in time".to_string());
            }
            Err(e) => return Err(format!("the new binary could not be waited for: {e}")),
        }
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "android")))))]
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

/// Whether an upgrade is on trial: the note of it is in the cache until a
/// start has lived its minute.
pub fn on_trial(cache: &Path) -> bool {
    cache.join(NOTE).is_file()
}

/// How long a display started by the one before it waits before it looks
/// for its page's port, so the one before has left it.
const HANDOVER: Duration = Duration::from_millis(1500);
const HANDOVER_SAID: &str = "GLASS_AFTER_UPGRADE";

/// Become the binary now in this one's place, with the arguments this one
/// was started with. On Unix the process is replaced and this returns only
/// where that failed (through `execv` directly, for the reason
/// `version_said` gives). On Windows, where a process cannot be replaced,
/// the new one is started on its own and this one is to leave at once: the
/// new one is told to wait a moment for the page's port.
#[cfg(all(unix, not(target_os = "android")))]
pub fn become_new(exe: &Path) -> Result<(), String> {
    use std::ffi::CString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let Ok(program) = CString::new(exe.as_os_str().as_bytes()) else {
        return Err(format!("{}: not a path a program can have", exe.display()));
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
    Err(format!(
        "{}: {}",
        exe.display(),
        std::io::Error::last_os_error()
    ))
}

#[cfg(windows)]
pub fn become_new(exe: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(HANDOVER_SAID, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", exe.display()))
}

#[cfg(not(any(windows, all(unix, not(target_os = "android")))))]
pub fn become_new(exe: &Path) -> Result<(), String> {
    Err(format!("{}: not on this system yet", exe.display()))
}

/// At a start: a display started by the one before it, after an upgrade,
/// waits a moment before anything else, so the one before has gone and its
/// page's port is free.
pub fn after_handover() {
    if std::env::var_os(HANDOVER_SAID).is_some() {
        std::thread::sleep(HANDOVER);
    }
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
        assert!(!release.test, "the latest is never a test release");
    }

    #[test]
    fn a_remote_on_test_releases_is_offered_the_newest_of_the_last_ten() {
        let digest = format!("sha256:{}", "ab".repeat(32));
        let home = "https://github.com/foonerd/glass/releases/download/";
        let at = |v: &str| format!("{home}v{v}/glass-{v}-x64.tar.gz");
        let named = |v: &str, test: bool, draft: bool| {
            let mut body = answer(
                &format!("v{v}"),
                &format!("glass-{v}-x64.tar.gz"),
                &at(v),
                5,
                &digest,
            );
            body["prerelease"] = Value::Bool(test);
            body["draft"] = Value::Bool(draft);
            body
        };
        // The newest by version wins, a test release among them; a draft
        // and a release without this machine's archive are passed over.
        let list = vec![
            named("0.9.6", false, false),
            named("0.9.7", true, false),
            named("0.9.9", true, true),
            answer(
                "v0.9.8",
                "glass-0.9.8-armv7.tar.gz",
                &at("0.9.8"),
                5,
                &digest,
            ),
            named("0.9.5", false, false),
        ];
        let release = newest_of(&list, &glass(), &["x64"], true).expect("a release");
        assert_eq!((release.version.as_str(), release.test), ("0.9.7", true));
        // The same list with the test release made the latest: still the newest.
        let plain: Vec<Value> = list
            .iter()
            .cloned()
            .map(|mut b| {
                b["prerelease"] = Value::Bool(false);
                b
            })
            .collect();
        let release = newest_of(&plain, &glass(), &["x64"], true).expect("a release");
        assert_eq!((release.version.as_str(), release.test), ("0.9.7", false));
        // Nothing for this machine in any of them: none.
        assert!(newest_of(&list, &glass(), &["armv8"], true)
            .unwrap_err()
            .contains("no release"));
        assert!(newest_of(&[], &glass(), &["x64"], true).is_err());
        // The latest alone never takes a test release; the list does.
        assert!(release_of(&named("0.9.7", true, false), &glass(), &["x64"], true).is_err());
        assert!(release_from(&named("0.9.7", true, false), &glass(), &["x64"], true, true).is_ok());
        assert!(
            release_from(&named("0.9.7", false, true), &glass(), &["x64"], true, true).is_err(),
            "a draft is never offered"
        );
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
            test: false,
            asset: String::new(),
            sums_url: None,
            sig_url: None,
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

    /// A zip of one stored and one deflated file, as `zip` writes them.
    fn zip_of(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut table = Vec::new();
        for (name, data, deflate) in files {
            let packed: Vec<u8> = if *deflate {
                let mut z =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                z.write_all(data).unwrap();
                z.finish().unwrap()
            } else {
                data.to_vec()
            };
            let at = out.len() as u32;
            let method: u16 = if *deflate { 8 } else { 0 };
            let mut local = vec![b'P', b'K', 3, 4, 20, 0, 0, 0];
            local.extend_from_slice(&method.to_le_bytes());
            local.extend_from_slice(&[0; 8]);
            local.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            local.extend_from_slice(&(data.len() as u32).to_le_bytes());
            local.extend_from_slice(&(name.len() as u16).to_le_bytes());
            local.extend_from_slice(&[0, 0]);
            local.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&local);
            out.extend_from_slice(&packed);
            let mut entry = vec![b'P', b'K', 1, 2, 20, 0, 20, 0, 0, 0];
            entry.extend_from_slice(&method.to_le_bytes());
            entry.extend_from_slice(&[0; 8]);
            entry.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            entry.extend_from_slice(&(data.len() as u32).to_le_bytes());
            entry.extend_from_slice(&(name.len() as u16).to_le_bytes());
            entry.extend_from_slice(&[0; 12]);
            entry.extend_from_slice(&at.to_le_bytes());
            entry.extend_from_slice(name.as_bytes());
            table.extend_from_slice(&entry);
        }
        let table_at = out.len() as u32;
        out.extend_from_slice(&table);
        let mut end = vec![b'P', b'K', 5, 6, 0, 0, 0, 0];
        end.extend_from_slice(&(files.len() as u16).to_le_bytes());
        end.extend_from_slice(&(files.len() as u16).to_le_bytes());
        end.extend_from_slice(&(table.len() as u32).to_le_bytes());
        end.extend_from_slice(&table_at.to_le_bytes());
        end.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&end);
        out
    }

    #[test]
    fn a_windows_archive_is_a_zip_with_the_display_under_bin() {
        assert_eq!(
            archive_name("glass-", "0.9.0", &["windows-x64"]).as_deref(),
            Some("glass-0.9.0-windows-x64.zip")
        );
        assert_eq!(
            archive_name("glass-evo-", "0.2.0", &["armv7", "arm"]).as_deref(),
            Some("glass-evo-0.2.0-armv7.tar.gz")
        );
        assert_eq!(
            archive_name("glass-", "0.9.0", &["android"]).as_deref(),
            Some("glass-0.9.0-android.apk"),
            "an Android release is one package"
        );
        let paths = binary_paths("glass-evo", &["windows-x64"]);
        assert_eq!(paths, vec!["bin/glass-evo.exe"]);
        let big = vec![9u8; 5000];
        let archive = zip_of(&[
            (
                "glass-evo-0.2.0-windows-x64/bin/SDL2.dll",
                b"a library",
                false,
            ),
            ("glass-evo-0.2.0-windows-x64/bin/glass-evo.exe", &big, true),
            (
                "glass-evo-0.2.0-windows-x64/remote/windows/install.ps1",
                b"# installer",
                true,
            ),
        ]);
        assert_eq!(binary_from(&archive, &paths).unwrap(), big);
        assert_eq!(
            file_from_zip(&archive, &["bin/SDL2.dll".to_string()]).unwrap(),
            b"a library",
            "a stored file too"
        );
        assert!(
            binary_from(&archive, &binary_paths("glass", &["windows-x64"]))
                .unwrap_err()
                .contains("holds no")
        );
        assert!(file_from_zip(b"PK but nothing of a zip", &paths).is_err());
        assert!(
            file_from_zip(&archive[..archive.len() - 30], &paths).is_err(),
            "cut short"
        );
        // A tar is still read as a tar.
        let tar = tar_gz(&[("x/bin/x64/glass", b"the display")]);
        assert_eq!(
            binary_from(&tar, &binary_paths("glass", &["x64"])).unwrap(),
            b"the display"
        );
    }

    #[test]
    fn what_is_kept_beside_a_program_keeps_its_ending() {
        assert_eq!(
            beside(Path::new("/home/u/.local/bin/glass"), ".prev"),
            Path::new("/home/u/.local/bin/glass.prev")
        );
        assert_eq!(
            beside(Path::new("C:/P/glass.exe"), ".prev"),
            Path::new("C:/P/glass.prev.exe")
        );
        assert_eq!(
            beside(Path::new("C:/P/GLASS.EXE"), ".new"),
            Path::new("C:/P/GLASS.new.EXE")
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
            test: false,
            asset: String::new(),
            sums_url: None,
            sig_url: None,
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
