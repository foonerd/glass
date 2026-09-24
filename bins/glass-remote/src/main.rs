//! Remote end of the line. UDP in, then the same plot, expose, and pane.
//! The UDP source is not connected yet, so this process plots an idle scene.

use expose::raster;
use intake::{IdleSource, Source};
use lead::SkinDesc;
use plot::step;

fn main() {
    let mut source = IdleSource;
    let skin = SkinDesc::basic();

    let input = source.poll();
    let scene = step(&skin, &input);
    let _ = raster(&scene);
}
