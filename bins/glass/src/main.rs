//! Player end of the line.
//!
//! `--once` reads the pipes, plots, and rasters a single frame.
//! `--output frame.ppm` writes that frame. Without it, the process keeps the
//! latest scene in memory at about 30 steps a second.

use std::env;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use expose::raster;
use intake::{PipeSource, Source};
use lead::SkinDesc;
use pane::{publish, show, write_ppm};
use plot::step;

fn main() -> ExitCode {
    let mut once = false;
    let mut print_scene = false;
    let mut output: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--once" => once = true,
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
                    "glass [--once] [--print] [--output frame.ppm]\n\
                     Reads {meter} and {spectrum}.",
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
    let window_open = output.is_some();
    let serving_remote = false;

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
        if window_open {
            let frame = raster(&scene);
            show(&frame);
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
        thread::sleep(Duration::from_millis(33));
    }
    ExitCode::SUCCESS
}
