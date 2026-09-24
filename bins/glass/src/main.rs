//! Player end of the line.
//!
//! `--once` reads the pipes, plots, and rasters a single frame.
//! `--output frame.ppm` writes that frame. Without it, the process keeps the
//! latest scene in memory at about 30 steps a second.

use std::env;
use std::process::ExitCode;
use std::thread;

use expose::raster;
use intake::{PipeSource, Source};
use lead::{frame_period, SkinDesc};
use pane::{publish, write_ppm, Surface};
use plot::step;

fn main() -> ExitCode {
    let mut once = false;
    let mut headless = false;
    let mut print_scene = false;
    let mut output: Option<String> = None;
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
            "--help" => {
                println!(
                    "glass [--once] [--headless] [--print] [--output frame.ppm]\n\
                     Reads {meter} and {spectrum}.\n\
                     A window opens when DISPLAY is set. --headless skips it.\n\
                     --output writes a PPM and still rasters.",
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
    let skin = SkinDesc::basic();
    let frame_rate = intake::installed_frame_rate();
    let period = frame_period(frame_rate);
    println!("glass: frame.rate={frame_rate}");
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
        if print_scene {
            println!(
                "left {:.2} right {:.2} bars {}",
                scene.left,
                scene.right,
                scene.bars.len()
            );
        }
        if surface.is_some() || write_file {
            let frame = raster(&scene);
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
