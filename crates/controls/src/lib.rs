//! A meter's controls under a finger: where the theme's buttons and
//! indicators are drawn, which one a point lands on, and what a tap or a
//! drag asks of the player. The display on the player, the remotes and
//! the browser face drive the same [`Touch`] with their pointer events and
//! send what comes back down the channel.

use std::time::Duration;

use expose::{Frame, IndicatorAssets};
use lead::{
    ButtonAction, GaugeSpec, InteractiveMode, Metadata, Moment, SkinDesc, StateIndicator, StateLook,
};
use plot::{Indicators, Scene};

/// What a tap on one of a theme's controls asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Tapped {
    Command(intake::Command),
    MeterNext,
    MeterPrevious,
    Dismiss,
}

/// Whether the controls act for this meter: the display's setting, and
/// with "as the theme says" the meter's own word or its having buttons.
pub fn interactive_now(skin: &SkinDesc) -> bool {
    match skin.run.interactive {
        InteractiveMode::On => true,
        InteractiveMode::Off => false,
        InteractiveMode::Theme => skin
            .indicators
            .as_ref()
            .is_some_and(|i| i.interactive || !i.buttons.is_empty()),
    }
}

fn inside(x: i32, y: i32, at: (i32, i32), size: (u32, u32)) -> bool {
    x >= at.0 && y >= at.1 && x < at.0 + size.0 as i32 && y < at.1 + size.1 as i32
}

/// Where along a gauge a point lies, 0 to 1: left to right along a wide
/// one, bottom to top along a tall one.
pub fn gauge_fraction(gauge: &GaugeSpec, x: i32, y: i32) -> f32 {
    let f = if gauge.w >= gauge.h {
        (x - gauge.x) as f32 / gauge.w.max(1) as f32
    } else {
        1.0 - (y - gauge.y) as f32 / gauge.h.max(1) as f32
    };
    f.clamp(0.0, 1.0)
}

/// The repeat that follows the player's: off, all, single, round again.
pub fn next_repeat(meta: &Metadata) -> &'static str {
    match (meta.repeat, meta.repeat_single) {
        (false, _) => "all",
        (true, false) => "single",
        (true, true) => "off",
    }
}

/// A bar being dragged.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub which: Which,
    pub gauge: GaugeSpec,
    /// Where along the bar the finger is, 0 to 1.
    pub value: f32,
    /// When the value was last sent, for a volume that follows the finger.
    sent: Option<Moment>,
    /// Whether the finger moved since it came down: a drag, else a tap.
    pub moved: bool,
}

/// How often a dragged volume goes to the player.
pub const DRAG_SEND_EVERY: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    Volume,
    Progress,
}

impl Drag {
    /// The command for where the finger is: the volume, or a seek by the
    /// track's length.
    pub fn command(&self, meta: &Metadata) -> intake::Command {
        match self.which {
            Which::Volume => intake::Command::with(
                "volume",
                serde_json::json!((self.value * 100.0).round() as u32),
            ),
            Which::Progress => intake::Command::with(
                "seek",
                serde_json::json!((self.value * meta.duration.max(0.0)).round() as u32),
            ),
        }
    }
}

/// A control of the meter on show: where it is drawn and what it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub at: (i32, i32),
    pub size: (u32, u32),
    pub kind: ControlKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ControlKind {
    Button(ButtonAction),
    PlayState,
    Mute,
    Shuffle,
    Repeat,
    Volume(GaugeSpec),
    Progress(GaugeSpec),
}

impl Control {
    pub fn gauge(&self) -> Option<(Which, &GaugeSpec)> {
        match &self.kind {
            ControlKind::Volume(g) => Some((Which::Volume, g)),
            ControlKind::Progress(g) => Some((Which::Progress, g)),
            _ => None,
        }
    }

    pub fn centre(&self) -> (i32, i32) {
        (
            self.at.0 + self.size.0 as i32 / 2,
            self.at.1 + self.size.1 as i32 / 2,
        )
    }

    /// The region a finger may land in: at least twice the margin on each
    /// axis, centred on the drawn box, and a bar reaching half a margin
    /// past both ends so its extremes can be hit.
    pub fn grown(&self, margin: u32) -> ((i32, i32), (u32, u32)) {
        let want = margin * 2;
        let (mut x, mut y) = self.at;
        let (mut w, mut h) = self.size;
        if w < want {
            x -= (want - w) as i32 / 2;
            w = want;
        }
        if h < want {
            y -= (want - h) as i32 / 2;
            h = want;
        }
        if self.gauge().is_some() {
            let reach = (margin / 2) as i32;
            if self.size.0 >= self.size.1 {
                x -= reach;
                w += 2 * reach as u32;
            } else {
                y -= reach;
                h += 2 * reach as u32;
            }
        }
        ((x, y), (w, h))
    }

    /// What a tap on this control asks for.
    pub fn tapped(&self, meta: &Metadata, x: i32, y: i32) -> Tapped {
        let command = |name: &str| Tapped::Command(intake::Command::new(name));
        let with = |name: &str, value: serde_json::Value| {
            Tapped::Command(intake::Command::with(name, value))
        };
        match &self.kind {
            ControlKind::Button(action) => match action {
                ButtonAction::Toggle => command("toggle"),
                ButtonAction::Play => command("play"),
                ButtonAction::Pause => command("pause"),
                ButtonAction::Stop => command("stop"),
                ButtonAction::Next => command("next"),
                ButtonAction::Previous => command("previous"),
                ButtonAction::MeterNext => Tapped::MeterNext,
                ButtonAction::MeterPrevious => Tapped::MeterPrevious,
                ButtonAction::Mute => with("volume", serde_json::json!("toggle")),
                ButtonAction::Random => with("random", serde_json::json!(!meta.random)),
                ButtonAction::Repeat => with("repeat", serde_json::json!(next_repeat(meta))),
                ButtonAction::Dismiss => Tapped::Dismiss,
            },
            ControlKind::PlayState => command("toggle"),
            ControlKind::Mute => with("volume", serde_json::json!("toggle")),
            ControlKind::Shuffle => with("random", serde_json::json!(!meta.random)),
            ControlKind::Repeat => with("repeat", serde_json::json!(next_repeat(meta))),
            ControlKind::Volume(gauge) => with(
                "volume",
                serde_json::json!((gauge_fraction(gauge, x, y) * 100.0).round() as u32),
            ),
            ControlKind::Progress(gauge) => with(
                "seek",
                serde_json::json!(
                    (gauge_fraction(gauge, x, y) * meta.duration.max(0.0)).round() as u32
                ),
            ),
        }
    }
}

/// The controls of the meter on show, in the order a tap is tested: the
/// theme's buttons, the play state, mute, shuffle and repeat indicators
/// (a LED's own size, a picture's size in its state), then the volume
/// and progress bars.
pub fn controls_of(indicators: &Indicators, assets: Option<&IndicatorAssets>) -> Vec<Control> {
    let spec = &indicators.spec;
    let mut controls = Vec::new();
    for (i, button) in spec.buttons.iter().enumerate() {
        let picture = assets
            .and_then(|a| a.buttons.get(i))
            .and_then(|p| p.as_ref());
        let size = if button.w > 0 && button.h > 0 {
            (button.w, button.h)
        } else {
            picture.map(|p| (p.width, p.height)).unwrap_or((0, 0))
        };
        if size.0 > 0 && size.1 > 0 {
            controls.push(Control {
                at: (button.x, button.y),
                size,
                kind: ControlKind::Button(button.action),
            });
        }
    }
    let state = |indicator: &Option<StateIndicator>,
                 frames: Option<&Vec<Option<Frame>>>,
                 state: usize,
                 kind: ControlKind|
     -> Option<Control> {
        let indicator = indicator.as_ref()?;
        let size = match &indicator.look {
            StateLook::Led { w, h, .. } => (*w, *h),
            StateLook::Icons { .. } => frames
                .and_then(|f| {
                    f.get(state)
                        .and_then(|p| p.as_ref())
                        .or_else(|| f.iter().flatten().next())
                })
                .map(|p| (p.width, p.height))
                .unwrap_or((48, 48)),
        };
        Some(Control {
            at: (indicator.x, indicator.y),
            size,
            kind,
        })
    };
    controls.extend(state(
        &spec.playstate,
        assets.map(|a| &a.playstate),
        indicators.play_state,
        ControlKind::PlayState,
    ));
    controls.extend(state(
        &spec.mute,
        assets.map(|a| &a.mute),
        indicators.mute_state,
        ControlKind::Mute,
    ));
    controls.extend(state(
        &spec.shuffle,
        assets.map(|a| &a.shuffle),
        indicators.shuffle_state,
        ControlKind::Shuffle,
    ));
    controls.extend(state(
        &spec.repeat,
        assets.map(|a| &a.repeat),
        indicators.repeat_state,
        ControlKind::Repeat,
    ));
    if let Some(gauge) = &spec.volume {
        controls.push(Control {
            at: (gauge.x, gauge.y),
            size: (gauge.w, gauge.h),
            kind: ControlKind::Volume(gauge.clone()),
        });
    }
    if let Some(gauge) = &spec.progress {
        controls.push(Control {
            at: (gauge.x, gauge.y),
            size: (gauge.w, gauge.h),
            kind: ControlKind::Progress(gauge.clone()),
        });
    }
    controls
}

/// The control under a point: the first whose drawn box holds it, else the
/// nearest whose grown box does, so a finger a little off a thin bar or a
/// small light still lands on it.
pub fn control_at(controls: &[Control], x: i32, y: i32, margin: u32) -> Option<&Control> {
    if let Some(exact) = controls.iter().find(|c| inside(x, y, c.at, c.size)) {
        return Some(exact);
    }
    controls
        .iter()
        .filter(|c| {
            let (at, size) = c.grown(margin);
            inside(x, y, at, size)
        })
        .min_by_key(|c| {
            let (cx, cy) = c.centre();
            (cx - x).pow(2) + (cy - y).pow(2)
        })
}

/// A pointer event in the frame's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pointer {
    pub kind: PointerKind,
    pub x: i32,
    pub y: i32,
}

/// What a pointer event asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// A command for the player, down the channel.
    Command(intake::Command),
    /// The rotation stepped: forward, or back.
    MeterStep(i32),
    /// What `exit.on.touch` does.
    Dismiss,
}

/// What came of a pointer event.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reaction {
    pub acts: Vec<Act>,
    /// A lift that ended a drag or landed on a control other than a
    /// dismiss: the touch was the controls' and not the touch rules'.
    pub taken: bool,
    /// The lift ended a drag rather than a tap, for a log line.
    pub dragged: bool,
}

/// The finger's effect on the controls between frames: a finger down on a
/// bar starts a drag, moves follow it (a volume goes to the player every
/// 150 ms), a lift ends it, or is a tap on what lies under it.
#[derive(Clone, Debug, Default)]
pub struct Touch {
    drag: Option<Drag>,
}

impl Touch {
    pub fn new() -> Self {
        Self::default()
    }

    /// The bar being dragged, if any: its value stands over the scene's.
    pub fn drag(&self) -> Option<&Drag> {
        self.drag.as_ref()
    }

    /// One pointer event against the meter's controls, `margin` the
    /// meter's touch margin.
    pub fn pointer(
        &mut self,
        event: Pointer,
        controls: &[Control],
        margin: u32,
        meta: &Metadata,
    ) -> Reaction {
        let mut reaction = Reaction::default();
        let hit = control_at(controls, event.x, event.y, margin);
        match event.kind {
            PointerKind::Down => {
                if let Some((which, gauge)) = hit.and_then(Control::gauge) {
                    self.drag = Some(Drag {
                        which,
                        value: gauge_fraction(gauge, event.x, event.y),
                        gauge: gauge.clone(),
                        sent: None,
                        moved: false,
                    });
                }
            }
            PointerKind::Move => {
                if let Some(d) = self.drag.as_mut() {
                    d.value = gauge_fraction(&d.gauge, event.x, event.y);
                    d.moved = true;
                    let due = d.sent.is_none_or(|at| at.elapsed() >= DRAG_SEND_EVERY);
                    if d.which == Which::Volume && due {
                        d.sent = Some(Moment::now());
                        reaction.acts.push(Act::Command(d.command(meta)));
                    }
                }
            }
            PointerKind::Up => {
                if let Some(mut d) = self.drag.take() {
                    d.value = gauge_fraction(&d.gauge, event.x, event.y);
                    reaction.acts.push(Act::Command(d.command(meta)));
                    reaction.taken = true;
                    reaction.dragged = d.moved;
                } else if let Some(control) = hit {
                    match control.tapped(meta, event.x, event.y) {
                        Tapped::Command(command) => {
                            reaction.acts.push(Act::Command(command));
                            reaction.taken = true;
                        }
                        Tapped::MeterNext => {
                            reaction.acts.push(Act::MeterStep(1));
                            reaction.taken = true;
                        }
                        Tapped::MeterPrevious => {
                            reaction.acts.push(Act::MeterStep(-1));
                            reaction.taken = true;
                        }
                        Tapped::Dismiss => reaction.acts.push(Act::Dismiss),
                    }
                }
            }
        }
        reaction
    }
}

/// The dragged bar's value over the scene's, so the knob follows the
/// finger before the player has answered.
pub fn override_scene(drag: Option<&Drag>, scene: &mut Scene) {
    if let (Some(d), Some(indicators)) = (drag, scene.indicators.as_mut()) {
        let value = (d.value * 100.0).round() as u32;
        match d.which {
            Which::Volume => indicators.volume = value,
            Which::Progress => indicators.progress = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn led(x: i32, y: i32, kind: ControlKind) -> Control {
        Control {
            at: (x, y),
            size: (10, 10),
            kind,
        }
    }

    fn volume_bar() -> (GaugeSpec, Vec<Control>) {
        let spec = lead::meter_indicators(
            "[m]\nconfig.extend = True\nvolume.pos = 0,300\nvolume.dim = 200,4\nvolume.style = slider\n",
            "m",
            "/t",
        )
        .expect("extended");
        let gauge = spec.volume.clone().expect("a volume bar");
        let controls = vec![
            led(100, 100, ControlKind::Mute),
            led(130, 100, ControlKind::Shuffle),
            Control {
                at: (gauge.x, gauge.y),
                size: (gauge.w, gauge.h),
                kind: ControlKind::Volume(gauge.clone()),
            },
        ];
        (gauge, controls)
    }

    /// A finger a little off a small light or a thin bar still lands on
    /// it, the nearest one when two are close; a drawn box wins outright.
    #[test]
    fn a_finger_lands_on_the_nearest_control_within_the_margin() {
        let (gauge, controls) = volume_bar();
        assert_eq!((gauge.x, gauge.y, gauge.w, gauge.h), (0, 300, 200, 4));
        assert_eq!(
            control_at(&controls, 105, 105, 24).map(|c| &c.kind),
            Some(&ControlKind::Mute)
        );
        assert_eq!(
            control_at(&controls, 95, 120, 24).map(|c| &c.kind),
            Some(&ControlKind::Mute)
        );
        assert_eq!(
            control_at(&controls, 128, 90, 24).map(|c| &c.kind),
            Some(&ControlKind::Shuffle)
        );
        assert_eq!(control_at(&controls, 95, 120, 0), None);
        let hit = control_at(&controls, 100, 282, 24).expect("the bar");
        assert!(matches!(hit.kind, ControlKind::Volume(_)));
        assert!(
            control_at(&controls, -8, 301, 24).is_some(),
            "half a margin past the end"
        );
        assert!(control_at(&controls, -30, 301, 24).is_none());
        assert_eq!(gauge_fraction(&gauge, 100, 301), 0.5);
        assert_eq!(gauge_fraction(&gauge, -8, 301), 0.0);
        assert_eq!(gauge_fraction(&gauge, 250, 301), 1.0);
        assert_eq!(controls[0].grown(24), ((81, 81), (48, 48)));
    }

    fn at(kind: PointerKind, x: i32, y: i32) -> Pointer {
        Pointer { kind, x, y }
    }

    /// A finger down on the bar drags it: the first move sends the volume
    /// at once, the next within 150 ms does not, the lift sends where it
    /// ended and the drag stands over the scene meanwhile.
    #[test]
    fn a_drag_follows_the_finger_and_sends_the_volume_as_it_goes() {
        let (_, controls) = volume_bar();
        let meta = Metadata::default();
        let mut touch = Touch::new();
        let down = touch.pointer(at(PointerKind::Down, 50, 301), &controls, 24, &meta);
        assert!(down.acts.is_empty() && !down.taken);
        let moved = touch.pointer(at(PointerKind::Move, 100, 301), &controls, 24, &meta);
        assert_eq!(
            moved.acts,
            vec![Act::Command(intake::Command::with(
                "volume",
                serde_json::json!(50)
            ))]
        );
        let again = touch.pointer(at(PointerKind::Move, 120, 301), &controls, 24, &meta);
        assert!(again.acts.is_empty(), "within 150 ms nothing more goes");
        let spec = lead::meter_indicators(
            "[m]\nconfig.extend = True\nvolume.pos = 0,300\nvolume.dim = 200,4\nvolume.style = slider\n",
            "m",
            "/t",
        )
        .expect("extended");
        let mut scene = Scene {
            indicators: Some(Indicators {
                spec,
                volume: 10,
                mute_state: 0,
                shuffle_state: 0,
                repeat_state: 0,
                play_state: 0,
                progress: 0,
            }),
            ..Scene::default()
        };
        override_scene(touch.drag(), &mut scene);
        assert_eq!(scene.indicators.as_ref().map(|i| i.volume), Some(60));
        let up = touch.pointer(at(PointerKind::Up, 150, 301), &controls, 24, &meta);
        assert_eq!(
            up.acts,
            vec![Act::Command(intake::Command::with(
                "volume",
                serde_json::json!(75)
            ))]
        );
        assert!(up.taken && up.dragged);
        assert!(touch.drag().is_none());
    }

    /// A lift on a light is a tap with its command; a lift on nothing is
    /// not taken; a dismiss button is not taken either, so the touch rules
    /// answer it.
    #[test]
    fn a_tap_commands_and_a_lift_on_nothing_is_left_to_the_touch_rules() {
        let (_, mut controls) = volume_bar();
        controls.push(led(300, 100, ControlKind::Button(ButtonAction::Dismiss)));
        let meta = Metadata {
            random: false,
            ..Metadata::default()
        };
        let mut touch = Touch::new();
        let tap = touch.pointer(at(PointerKind::Up, 132, 104), &controls, 24, &meta);
        assert_eq!(
            tap.acts,
            vec![Act::Command(intake::Command::with(
                "random",
                serde_json::json!(true)
            ))]
        );
        assert!(tap.taken && !tap.dragged);
        let nothing = touch.pointer(at(PointerKind::Up, 500, 500), &controls, 24, &meta);
        assert!(nothing.acts.is_empty() && !nothing.taken);
        let dismiss = touch.pointer(at(PointerKind::Up, 305, 105), &controls, 24, &meta);
        assert_eq!(dismiss.acts, vec![Act::Dismiss]);
        assert!(!dismiss.taken);
    }
}
