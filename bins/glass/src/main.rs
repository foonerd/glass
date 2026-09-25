//! Player end of the line.
//!
//! `--once` reads the pipes, plots, and rasters a single frame.
//! `--output frame.ppm` writes that frame. Without it, the process keeps the
//! latest scene in memory at about 30 steps a second.

use std::env;
use std::process::ExitCode;
use std::thread;

use expose::{raster_over, read_png, Fonts, Stack};
use intake::{PipeSource, Source};
use lead::{frame_period, Input, SkinDesc};
use pane::{publish, write_ppm, Surface};
use plot::{step, Scene};

fn load_theme(dir: &str, file: &str) -> Option<expose::Frame> {
    if dir.is_empty() || file.is_empty() {
        return None;
    }
    read_png(std::path::Path::new(dir).join(file).as_path())
}

/// One recorded step, the form `plot` replays from `testdata/frames/`.
#[derive(serde::Serialize)]
struct Recorded<'a> {
    skin: &'a SkinDesc,
    input: &'a Input,
    scene: &'a Scene,
}

fn main() -> ExitCode {
    let mut once = false;
    let mut headless = false;
    let mut print_scene = false;
    let mut output: Option<String> = None;
    let mut record: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--once" => once = true,
            "--headless" => headless = true,
            "--print" => print_scene = true,
            "--output" => match args.next() {
                Some(path) => output = Some(path),
                None => {
                    eprintln!("glass: --output needs a path");
                    return ExitCode::from(2);
                }
            },
            "--record" => match args.next() {
                Some(path) => record = Some(path),
                None => {
                    eprintln!("glass: --record needs a path");
                    return ExitCode::from(2);
                }
            },
            "--help" => {
                println!(
                    "glass [--once] [--headless] [--print] [--output frame.ppm] [--record step.json]\n\
                     Reads {meter} and {spectrum}.\n\
                     A window opens when DISPLAY is set. --headless skips it.\n\
                     --output writes a PPM and still rasters.\n\
                     --record writes the skin, input and scene of each step as JSON.",
                    meter = lead::METER_FIFO,
                    spectrum = lead::SPECTRUM_FIFO
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("glass: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    let mut source = PipeSource::installed();
    let skin = intake::installed_skin();
    let frame_rate = intake::installed_frame_rate();
    let period = frame_period(frame_rate);
    println!(
        "glass: frame.rate={frame_rate} size={}x{} theme={}",
        skin.width,
        skin.height,
        if skin.theme_dir.is_empty() {
            "none"
        } else {
            &skin.theme_dir
        }
    );
    let background = load_theme(&skin.theme_dir, &skin.background);
    let face = load_theme(&skin.theme_dir, &skin.face);
    let front = load_theme(&skin.theme_dir, &skin.front);
    let indicator = load_theme(&skin.theme_dir, &skin.indicator);
    let fonts = Fonts::load(&skin.fonts);
    println!("glass: fonts loaded {} of 4", fonts.loaded());
    let show_window = env::var_os("DISPLAY").is_some() && !headless;
    let write_file = output.is_some();
    let serving_remote = false;
    let mut surface = if show_window {
        match Surface::open(skin.width, skin.height) {
            Ok(surface) => Some(surface),
            Err(err) => {
                eprintln!("glass: {err}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };

    loop {
        let input = source.poll();
        let scene = step(&skin, &input);
        if let Some(path) = &record {
            let recorded = Recorded {
                skin: &skin,
                input: &input,
                scene: &scene,
            };
            let written = serde_json::to_string_pretty(&recorded)
                .map_err(|err| err.to_string())
                .and_then(|text| std::fs::write(path, text).map_err(|err| err.to_string()));
            if let Err(err) = written {
                eprintln!("glass: --record {path}: {err}");
                return ExitCode::from(1);
            }
        }
        if print_scene {
            println!(
                "left {:.2} right {:.2} bars {}",
                scene.left,
                scene.right,
                scene.bars.len()
            );
        }
        if surface.is_some() || write_file {
            let frame = raster_over(
                &scene,
                Stack {
                    screen: background.as_ref(),
                    face: face.as_ref(),
                    front: front.as_ref(),
                    needle: indicator.as_ref(),
                    face_at: skin.face_at,
                    fonts: Some(&fonts),
                },
            );
            if let Some(window) = surface.as_mut() {
                match window.show(&frame) {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(err) => {
                        eprintln!("glass: {err}");
                        return ExitCode::from(1);
                    }
                }
            }
            if let Some(path) = &output {
                if let Err(err) = write_ppm(path, &frame) {
                    eprintln!("glass: {err}");
                    return ExitCode::from(1);
                }
            }
        }
        if serving_remote {
            publish(&scene);
        }
        if once {
            break;
        }
        thread::sleep(period);
    }
    ExitCode::SUCCESS
}
