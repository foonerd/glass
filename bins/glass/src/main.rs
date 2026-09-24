//! Player end of the line. Headless skips the raster.

use expose::raster;
use intake::{IdleSource, Source};
use lead::SkinDesc;
use pane::{publish, show};
use plot::step;

fn main() {
    let mut source = IdleSource;
    let skin = SkinDesc::default();
    let window_open = false;
    let serving_remote = false;

    let input = source.poll();
    let scene = step(&skin, &input);
    if window_open {
        let frame = raster(&scene);
        show(&frame);
    }
    if serving_remote {
        publish(&scene);
    }
}
