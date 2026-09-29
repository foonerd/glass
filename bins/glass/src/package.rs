//! The package: a theme as the catalogue takes it, `glass --package`. The
//! meters are snapshotted by the display itself, tiled into one preview,
//! and the theme, its spectrum twin and the preview go into a zip in the
//! catalogue's layout: `<name>/preview.png`, `<name>/templates/<name>/…`
//! and `<name>/templates_spectrum/<name>/…`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use expose::{fit_art, Blend, Frame};

/// The house colour behind the tiles of a preview.
const GUTTER: [u8; 4] = [58, 65, 80, 255];
/// The space between tiles and around them, in pixels.
const GAP: u32 = 2;
/// How many meters a preview shows at most.
const TILES_MAX: usize = 9;

/// How many columns a preview of `n` tiles takes.
pub fn columns(n: usize) -> u32 {
    match n {
        0 | 1 => 1,
        2 => 2,
        3 => 3,
        4 => 2,
        _ => 3,
    }
}

/// The tiles side by side, `width` pixels wide in all: one tile is the
/// picture itself; more are scaled to the columns' width, their shape
/// kept, in rows on the house colour with a gap between; past nine, the
/// first nine.
pub fn montage(tiles: &[Frame], width: u32) -> Frame {
    let tiles: Vec<&Frame> = tiles.iter().take(TILES_MAX).collect();
    let Some(first) = tiles.first() else {
        return Frame {
            blend: Blend::Normal,
            width: width.max(1),
            height: 1,
            rgba: GUTTER.repeat(width.max(1) as usize),
        };
    };
    if tiles.len() == 1 {
        return (*first).clone();
    }
    let cols = columns(tiles.len());
    let rows = (tiles.len() as u32).div_ceil(cols);
    let tile_w = ((width.max(cols * 8) - GAP * (cols + 1)) / cols).max(1);
    let tile_h = ((u64::from(tile_w) * u64::from(first.height.max(1)))
        / u64::from(first.width.max(1)))
    .max(1) as u32;
    let height = rows * tile_h + GAP * (rows + 1);
    let mut out = Frame {
        blend: Blend::Normal,
        width,
        height,
        rgba: GUTTER.repeat((width * height) as usize),
    };
    for (i, tile) in tiles.iter().enumerate() {
        let scaled = fit_art(tile, tile_w, tile_h);
        let (col, row) = (i as u32 % cols, i as u32 / cols);
        let (x0, y0) = (GAP + col * (tile_w + GAP), GAP + row * (tile_h + GAP));
        for y in 0..tile_h {
            let dst = ((y0 + y) * width + x0) as usize * 4;
            let src = (y * tile_w) as usize * 4;
            out.rgba[dst..dst + tile_w as usize * 4]
                .copy_from_slice(&scaled.rgba[src..src + tile_w as usize * 4]);
        }
    }
    out
}

/// Every file under `dir`, dotfiles left out, as (path in the zip, file).
fn files_under(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let inside = format!("{prefix}/{name}");
        if path.is_dir() {
            files_under(&path, &inside, out)?;
        } else if path.is_file() {
            out.push((inside, path));
        }
    }
    Ok(())
}

/// The zip's entries for a theme: the preview, the theme folder and the
/// twin when there is one, under the theme's name.
pub fn entries(
    name: &str,
    theme: &Path,
    spectrum: Option<&Path>,
    preview: &Path,
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = vec![(format!("{name}/preview.png"), preview.to_path_buf())];
    files_under(theme, &format!("{name}/templates/{name}"), &mut out)?;
    if let Some(twin) = spectrum {
        files_under(twin, &format!("{name}/templates_spectrum/{name}"), &mut out)?;
    }
    Ok(out)
}

/// The zip written: each file deflated behind a local header, the
/// central directory after them, as any reader expects.
pub fn write_zip(zip: &Path, entries: &[(String, PathBuf)]) -> Result<(), String> {
    let mut body: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    for (name, path) in entries {
        let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let crc = crc32fast::hash(&data);
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&data)
            .and_then(|_| encoder.finish())
            .map(|packed| {
                let offset = body.len() as u32;
                let name_bytes = name.as_bytes();
                // The local header.
                body.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
                body.extend_from_slice(&20u16.to_le_bytes()); // version needed
                body.extend_from_slice(&0x0800u16.to_le_bytes()); // flags: names are UTF-8
                body.extend_from_slice(&8u16.to_le_bytes()); // deflate
                body.extend_from_slice(&0u16.to_le_bytes()); // time
                body.extend_from_slice(&0x21u16.to_le_bytes()); // date: 1980-01-01
                body.extend_from_slice(&crc.to_le_bytes());
                body.extend_from_slice(&(packed.len() as u32).to_le_bytes());
                body.extend_from_slice(&(data.len() as u32).to_le_bytes());
                body.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
                body.extend_from_slice(&0u16.to_le_bytes()); // extra
                body.extend_from_slice(name_bytes);
                body.extend_from_slice(&packed);
                // Its line in the central directory.
                central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
                central.extend_from_slice(&20u16.to_le_bytes()); // made by
                central.extend_from_slice(&20u16.to_le_bytes()); // version needed
                central.extend_from_slice(&0x0800u16.to_le_bytes());
                central.extend_from_slice(&8u16.to_le_bytes());
                central.extend_from_slice(&0u16.to_le_bytes());
                central.extend_from_slice(&0x21u16.to_le_bytes());
                central.extend_from_slice(&crc.to_le_bytes());
                central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
                central.extend_from_slice(&(data.len() as u32).to_le_bytes());
                central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
                central.extend_from_slice(&0u16.to_le_bytes()); // extra
                central.extend_from_slice(&0u16.to_le_bytes()); // comment
                central.extend_from_slice(&0u16.to_le_bytes()); // disk
                central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
                central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
                central.extend_from_slice(&offset.to_le_bytes());
                central.extend_from_slice(name_bytes);
            })
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let central_at = body.len() as u32;
    let central_len = central.len() as u32;
    body.extend_from_slice(&central);
    // The end of the central directory.
    body.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    body.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    body.extend_from_slice(&central_len.to_le_bytes());
    body.extend_from_slice(&central_at.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    fs::write(zip, body).map_err(|e| format!("{}: {e}", zip.display()))
}

/// The package built from the snapshots the display left under
/// `shots`, one PNG per meter: the meters' pictures in the theme's order
/// tiled into `preview.png`, then the zip beside them under `out`,
/// named after the theme. The zip's path comes back.
pub fn build(
    name: &str,
    theme: &Path,
    spectrum: Option<&Path>,
    shots: &Path,
    out: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    // The theme's own order of meters, so the preview reads as the theme does.
    let order: Vec<String> = fs::read_to_string(theme.join("meters.txt"))
        .map(|text| {
            text.lines()
                .filter_map(|l| {
                    let l = l.trim();
                    l.strip_prefix('[')
                        .and_then(|s| s.strip_suffix(']'))
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let mut tiles: Vec<Frame> = Vec::new();
    for meter in &order {
        let file = shots.join(format!("{}.png", meter.replace('/', "_")));
        let Some((w, h)) = expose::picture_size(&file) else {
            continue;
        };
        if let Some(frame) = expose::read_art(&file, w, h, None) {
            tiles.push(frame);
        }
    }
    if tiles.is_empty() {
        return Err(format!(
            "{}: no snapshot of any meter to make a preview from",
            shots.display()
        ));
    }
    let width = tiles[0].width;
    let preview = montage(&tiles, width);
    let preview_path = out.join(format!("{name}.preview.png"));
    expose::write_png(&preview_path, &preview)?;
    let twin = spectrum.map(Path::to_path_buf).or_else(|| {
        let beside = theme
            .parent()?
            .parent()?
            .join("templates_spectrum")
            .join(name);
        beside.join("spectrum.txt").is_file().then_some(beside)
    });
    let list = entries(name, theme, twin.as_deref(), &preview_path)?;
    let zip = out.join(format!("{name}.zip"));
    write_zip(&zip, &list)?;
    Ok(zip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Frame {
        Frame {
            blend: Blend::Normal,
            width: w,
            height: h,
            rgba: c.repeat((w * h) as usize),
        }
    }

    /// One tile is itself; three go side by side at a third of the width
    /// each, the gutter between; seven take three columns and three rows.
    #[test]
    fn a_preview_tiles_the_meters_by_their_count() {
        let one = montage(&[solid(100, 50, [1, 2, 3, 255])], 300);
        assert_eq!((one.width, one.height), (100, 50));
        let tiles = vec![
            solid(100, 50, [255, 0, 0, 255]),
            solid(100, 50, [0, 255, 0, 255]),
            solid(100, 50, [0, 0, 255, 255]),
        ];
        let three = montage(&tiles, 302);
        // Tiles of (302 - 8) / 3 = 98 wide, 49 high; two gaps of 2 around a row of 49.
        assert_eq!((three.width, three.height), (302, 53));
        let px = |f: &Frame, x: u32, y: u32| {
            let at = ((y * f.width + x) * 4) as usize;
            [f.rgba[at], f.rgba[at + 1], f.rgba[at + 2]]
        };
        assert_eq!(px(&three, 0, 0), [58, 65, 80], "the gutter");
        assert_eq!(px(&three, 2, 2), [255, 0, 0], "the first tile");
        assert_eq!(px(&three, 102, 2), [0, 255, 0], "the second, past a gap");
        assert_eq!(px(&three, 202, 2), [0, 0, 255], "the third");
        assert_eq!(columns(7), 3);
        let seven = montage(&vec![solid(100, 50, [9, 9, 9, 255]); 7], 302);
        assert_eq!(seven.height, 3 * 49 + 4 * 2);
        assert_eq!(
            montage(&vec![solid(10, 10, [0; 4]); 12], 100).height,
            montage(&vec![solid(10, 10, [0; 4]); 9], 100).height
        );
    }

    /// A theme folder and a twin into a zip: the entries under the theme's
    /// name in the catalogue's layout, each read back by a plain zip
    /// reader with its name, size and checksum, and its bytes inflating to
    /// what was written.
    #[test]
    fn a_zip_holds_the_theme_pair_and_the_preview_in_the_catalogue_layout() {
        let root = std::env::temp_dir().join(format!("glass-package-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let theme = root.join("t");
        let twin = root.join("s");
        fs::create_dir_all(theme.join("sub")).unwrap();
        fs::create_dir_all(&twin).unwrap();
        fs::write(theme.join("meters.txt"), b"[m]\nmeter.x = 1\n").unwrap();
        fs::write(theme.join("sub").join("a.bin"), b"abc".repeat(1000)).unwrap();
        fs::write(theme.join(".hidden"), b"no").unwrap();
        fs::write(twin.join("spectrum.txt"), b"[s]\n").unwrap();
        fs::write(root.join("preview.png"), b"png").unwrap();
        let entries =
            entries("1280x720_x", &theme, Some(&twin), &root.join("preview.png")).unwrap();
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "1280x720_x/preview.png",
                "1280x720_x/templates/1280x720_x/meters.txt",
                "1280x720_x/templates/1280x720_x/sub/a.bin",
                "1280x720_x/templates_spectrum/1280x720_x/spectrum.txt",
            ]
        );
        let zip = root.join("x.zip");
        write_zip(&zip, &entries).unwrap();
        let bytes = fs::read(&zip).unwrap();
        // The end record points at the central directory; walk it.
        let end = bytes.len() - 22;
        assert_eq!(&bytes[end..end + 4], &0x0605_4b50u32.to_le_bytes());
        let count = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as usize;
        let central_at = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        assert_eq!(count, 4);
        let mut at = central_at;
        let mut seen = Vec::new();
        for _ in 0..count {
            assert_eq!(&bytes[at..at + 4], &0x0201_4b50u32.to_le_bytes());
            let crc = u32::from_le_bytes(bytes[at + 16..at + 20].try_into().unwrap());
            let packed = u32::from_le_bytes(bytes[at + 20..at + 24].try_into().unwrap()) as usize;
            let size = u32::from_le_bytes(bytes[at + 24..at + 28].try_into().unwrap()) as usize;
            let name_len = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
            let offset = u32::from_le_bytes(bytes[at + 42..at + 46].try_into().unwrap()) as usize;
            let name = std::str::from_utf8(&bytes[at + 46..at + 46 + name_len])
                .unwrap()
                .to_string();
            // The local header at the offset, then the packed bytes.
            assert_eq!(&bytes[offset..offset + 4], &0x0403_4b50u32.to_le_bytes());
            let local_name_len =
                u16::from_le_bytes([bytes[offset + 26], bytes[offset + 27]]) as usize;
            let data_at = offset + 30 + local_name_len;
            let mut inflated = Vec::new();
            flate2::read::DeflateDecoder::new(&bytes[data_at..data_at + packed])
                .read_to_end(&mut inflated)
                .unwrap();
            assert_eq!(inflated.len(), size, "{name}");
            assert_eq!(crc32fast::hash(&inflated), crc, "{name}");
            seen.push((name, inflated));
            at += 46 + name_len;
        }
        assert_eq!(seen[1].0, "1280x720_x/templates/1280x720_x/meters.txt");
        assert_eq!(seen[1].1, b"[m]\nmeter.x = 1\n");
        assert_eq!(seen[2].1.len(), 3000);
        let _ = fs::remove_dir_all(&root);
    }
}
