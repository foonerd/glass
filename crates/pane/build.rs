//! The runtime package ships `libSDL2-2.0.so.0`. The dev package adds the
//! `libSDL2.so` name the linker asks for. When that name is missing, point
//! the link at the runtime library.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("sdl-link");
    let _ = fs::create_dir_all(&out);
    if let Some(runtime) = runtime_library() {
        let alias = out.join("libSDL2.so");
        let _ = fs::remove_file(&alias);
        if std::os::unix::fs::symlink(&runtime, &alias).is_ok() {
            println!("cargo:rustc-link-search=native={}", out.display());
        }
        println!("cargo:rerun-if-changed={}", runtime.display());
    }
}

fn runtime_library() -> Option<PathBuf> {
    let listed = Command::new("ldconfig").arg("-p").output().ok()?;
    let text = String::from_utf8_lossy(&listed.stdout);
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("libSDL2-2.0.so.0 ") {
            continue;
        }
        let path = line.rsplit(" => ").next()?.trim();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}
