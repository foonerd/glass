//! Remote end of the line. UDP in, then the same plot, expose, and pane.

use expose::raster;
use intake::{IdleSource, Source};
use lead::SkinDesc;
use pane::show;
use plot::step;

fn main() {
    let mut source = IdleSource;
    let skin = SkinDesc::default();

    let input = source.poll();
    let scene = step(&skin, &input);
    let frame = raster(&scene);
    show(&frame);
}
