//! A face laid over the picture, frame by frame: the one piece of composing
//! the display's loop and the browser's pipeline both do, kept here so the
//! screen and the views cannot come to differ in it.

use expose::Frame;

use crate::{Cover, Overlay, View};

/// What laying a face over a frame came to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lay {
    /// The face is over the picture: show `Laid::frame`, not the picture.
    pub drew: bool,
    /// It is the frame before's, pixel for pixel: nothing was drawn again.
    pub same: bool,
}

/// A face over the picture on a copy of its own, kept from frame to frame:
/// a new copy every frame is megabytes asked of the system sixty times a
/// second, and a face that stands over a picture that stands is not drawn
/// again at all.
#[derive(Default)]
pub struct Laid {
    copy: Option<Frame>,
    /// The copy holds what the face drew over the picture as it still stands.
    stands: bool,
}

impl Laid {
    /// Lay `face` over `base`. The face is asked what it covers: nothing,
    /// and the copy is spared; the same as before over the same picture
    /// (`base_same`: the base is the frame before's), and the copy is kept
    /// as it is; anything else, and the base is copied and the face draws
    /// on the copy.
    pub fn lay(
        &mut self,
        face: &mut dyn Overlay,
        base: &Frame,
        base_same: bool,
        view: &View,
    ) -> Lay {
        let fits = self.copy.as_ref().is_some_and(|own| {
            own.width == base.width
                && own.height == base.height
                && own.rgba.len() == base.rgba.len()
        });
        let lay = match face.covers(view) {
            Cover::Nothing => Lay {
                drew: false,
                same: false,
            },
            Cover::Same if fits && self.stands && base_same => Lay {
                drew: true,
                same: true,
            },
            _ => {
                let own = match self.copy.as_mut() {
                    Some(own) if fits => {
                        own.rgba.copy_from_slice(&base.rgba);
                        own.blend = base.blend;
                        own
                    }
                    _ => self.copy.insert(base.clone()),
                };
                Lay {
                    drew: face.draw(own, view),
                    same: false,
                }
            }
        };
        self.stands = lay.drew;
        lay
    }

    /// The picture with the face over it, after a `lay` that drew.
    pub fn frame(&self) -> Option<&Frame> {
        self.copy.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Wall;
    use std::collections::BTreeMap;

    /// A face that says what it is told to and counts its draws.
    struct Mark {
        cover: Cover,
        draws: u32,
    }

    impl Overlay for Mark {
        fn covers(&mut self, _view: &View) -> Cover {
            self.cover
        }
        fn draw(&mut self, frame: &mut Frame, _view: &View) -> bool {
            self.draws += 1;
            frame.rgba[0] = 200;
            true
        }
        fn pointer(&mut self, _kind: crate::PointerKind, _x: i32, _y: i32, _view: &View) -> bool {
            false
        }
        fn commands(&mut self) -> Vec<intake::Command> {
            Vec::new()
        }
    }

    fn picture(width: u32, height: u32, shade: u8) -> Frame {
        Frame {
            blend: expose::Blend::Normal,
            width,
            height,
            rgba: vec![shade; width as usize * height as usize * 4],
        }
    }

    #[test]
    fn a_face_is_drawn_on_a_copy_and_not_again_while_it_and_the_picture_stand() {
        let input = lead::Input::default();
        let fonts = expose::Fonts::default();
        let wall = Wall::default();
        let settings = BTreeMap::new();
        let view = View {
            input: &input,
            fonts: &fonts,
            width: 4,
            height: 2,
            now_ms: 0,
            wall: &wall,
            ours: true,
            scale: 1.0,
            settings: &settings,
            theme_dir: "",
        };
        let base = picture(4, 2, 9);
        let mut laid = Laid::default();
        let mut face = Mark {
            cover: Cover::New,
            draws: 0,
        };
        // Something new: drawn on a copy, the picture itself untouched.
        let lay = laid.lay(&mut face, &base, false, &view);
        assert_eq!(
            lay,
            Lay {
                drew: true,
                same: false
            }
        );
        assert_eq!((face.draws, base.rgba[0]), (1, 9));
        assert_eq!(laid.frame().map(|f| (f.rgba[0], f.rgba[1])), Some((200, 9)));
        // The same face over the same picture: nothing is drawn.
        face.cover = Cover::Same;
        let lay = laid.lay(&mut face, &base, true, &view);
        assert_eq!(
            lay,
            Lay {
                drew: true,
                same: true
            }
        );
        assert_eq!(face.draws, 1);
        // The same face over a picture that moved: drawn again, on the new picture.
        let moved = picture(4, 2, 30);
        let lay = laid.lay(&mut face, &moved, false, &view);
        assert_eq!(
            lay,
            Lay {
                drew: true,
                same: false
            }
        );
        assert_eq!(face.draws, 2);
        assert_eq!(
            laid.frame().map(|f| (f.rgba[0], f.rgba[1])),
            Some((200, 30))
        );
        // Nothing to draw: the copy is not touched, and "the same" after
        // that is drawn afresh, since the copy no longer stands.
        face.cover = Cover::Nothing;
        assert_eq!(
            laid.lay(&mut face, &moved, true, &view),
            Lay {
                drew: false,
                same: false
            }
        );
        face.cover = Cover::Same;
        assert_eq!(
            laid.lay(&mut face, &moved, true, &view),
            Lay {
                drew: true,
                same: false
            }
        );
        assert_eq!(face.draws, 3);
        // A picture of another size: a new copy, never the old one's bytes.
        let wide = picture(8, 2, 5);
        assert_eq!(
            laid.lay(&mut face, &wide, true, &view),
            Lay {
                drew: true,
                same: false
            }
        );
        assert_eq!(laid.frame().map(|f| (f.width, f.rgba.len())), Some((8, 64)));
    }
}
