//! The frame rate governor. When frames keep overrunning their period with
//! the painters already at their most, the target rate steps down a
//! ladder, so a theme too heavy for the player runs smoothly at a lower
//! rate instead of stuttering at the configured one. A step back up is
//! tried after a while, and the wait doubles each time the theme proves
//! too heavy again. A meter change starts over at the configured rate.
//! Nothing here touches the setting the user chose; the plugin shows the
//! rate in force beside it.

use std::time::Duration;

/// The rates a display may run at, from the top down.
pub const LADDER: [u32; 5] = [60, 45, 30, 20, 15];
/// Frames are judged over windows this long.
const WINDOW: Duration = Duration::from_secs(2);
/// The first seconds after a start, and after each change of rate, are not
/// judged: fresh pictures turn then, and the pipeline settles to the new
/// period. The next window opens when they are over.
const SETTLE: Duration = Duration::from_secs(3);
/// How long a lowered rate is kept before one step up is tried.
const FIRST_PROBE: Duration = Duration::from_secs(120);
/// The wait between probes doubles up to this.
const LONGEST_PROBE: Duration = Duration::from_secs(1800);

/// What the governor decided at the end of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Lowered(u32),
    Raised(u32),
}

pub struct Governor {
    on: bool,
    configured: u32,
    target: u32,
    judge_from: Duration,
    window_from: Duration,
    frames: u32,
    overruns: u32,
    lowered_at: Option<Duration>,
    probe_after: Duration,
}

impl Governor {
    /// A governor for a display configured at `configured` frames a second,
    /// judging from `now` (the display's clock since its start).
    pub fn new(configured: u32, on: bool, now: Duration) -> Self {
        Self {
            on,
            configured: configured.max(1),
            target: configured.max(1),
            judge_from: now + SETTLE,
            window_from: now + SETTLE,
            frames: 0,
            overruns: 0,
            lowered_at: None,
            probe_after: FIRST_PROBE,
        }
    }

    /// The rate in force.
    pub fn target(&self) -> u32 {
        self.target
    }

    /// A new meter: the configured rate again, judged afresh.
    pub fn reset(&mut self, now: Duration) {
        self.target = self.configured;
        self.settle(now);
        self.lowered_at = None;
        self.probe_after = FIRST_PROBE;
    }

    fn settle(&mut self, now: Duration) {
        self.judge_from = now + SETTLE;
        self.window_from = self.judge_from;
        self.frames = 0;
        self.overruns = 0;
    }

    fn below(&self, rate: u32) -> Option<u32> {
        LADDER.iter().copied().find(|r| *r < rate)
    }

    fn above(&self, rate: u32) -> Option<u32> {
        if rate >= self.configured {
            return None;
        }
        let step = LADDER
            .iter()
            .rev()
            .copied()
            .find(|r| *r > rate)
            .unwrap_or(self.configured);
        Some(step.min(self.configured))
    }

    /// One frame: whether it overran its period, and whether the painters
    /// were at their most while it did. A change comes at the end of a window.
    pub fn frame(&mut self, now: Duration, overran: bool, painters_maxed: bool) -> Option<Change> {
        if !self.on || now < self.judge_from {
            return None;
        }
        self.frames += 1;
        if overran && painters_maxed {
            self.overruns += 1;
        }
        if now < self.window_from + WINDOW {
            return None;
        }
        let (frames, overruns) = (self.frames, self.overruns);
        self.frames = 0;
        self.overruns = 0;
        self.window_from = now;
        if frames == 0 {
            return None;
        }
        if overruns * 4 > frames {
            let next = self.below(self.target)?;
            self.target = next;
            self.lowered_at = Some(now);
            self.settle(now);
            return Some(Change::Lowered(next));
        }
        let at = self.lowered_at?;
        if now < at + self.probe_after {
            return None;
        }
        let up = self.above(self.target)?;
        self.target = up;
        self.settle(now);
        self.probe_after = (self.probe_after * 2).min(LONGEST_PROBE);
        self.lowered_at = if up < self.configured {
            Some(now)
        } else {
            None
        };
        Some(Change::Raised(up))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    /// Feed `n` frames a second from `from` for `seconds`, each overrunning or not.
    fn run(g: &mut Governor, from: f64, seconds: f64, overran: bool, maxed: bool) -> Vec<Change> {
        let mut changes = Vec::new();
        let mut t = from;
        while t < from + seconds {
            if let Some(c) = g.frame(secs(t), overran, maxed) {
                changes.push(c);
            }
            t += 1.0 / 60.0;
        }
        changes
    }

    #[test]
    fn overruns_with_the_painters_at_their_most_lower_the_rate_step_by_step() {
        let mut g = Governor::new(60, true, secs(0.0));
        assert!(
            run(&mut g, 0.0, 2.5, true, true).is_empty(),
            "the first seconds are not judged"
        );
        // A window of two seconds, three seconds to settle after each step.
        let changes = run(&mut g, 2.5, 8.5, true, true);
        assert_eq!(changes, vec![Change::Lowered(45), Change::Lowered(30)]);
        assert_eq!(g.target(), 30);
    }

    #[test]
    fn overruns_while_painters_can_still_be_added_are_theirs_to_solve() {
        let mut g = Governor::new(60, true, secs(0.0));
        assert!(run(&mut g, 0.0, 10.0, true, false).is_empty());
        assert_eq!(g.target(), 60);
    }

    #[test]
    fn a_step_up_is_tried_after_the_wait_and_the_wait_doubles_when_it_fails() {
        let mut g = Governor::new(60, true, secs(0.0));
        let _ = run(&mut g, 0.0, 6.0, true, true);
        assert_eq!(g.target(), 45);
        // Quiet frames: nothing until two minutes have passed since the lowering.
        assert!(run(&mut g, 6.0, 100.0, false, true).is_empty());
        let raised = run(&mut g, 106.0, 30.0, false, true);
        assert_eq!(raised, vec![Change::Raised(60)]);
        // Too heavy again: lowered, and the next probe waits four minutes.
        let lowered = run(&mut g, 136.0, 3.0, true, true);
        assert_eq!(lowered, vec![Change::Lowered(45)]);
        assert!(
            run(&mut g, 139.0, 230.0, false, true).is_empty(),
            "no probe within the doubled wait"
        );
        assert_eq!(
            run(&mut g, 369.0, 20.0, false, true),
            vec![Change::Raised(60)]
        );
    }

    #[test]
    fn a_configured_rate_off_the_ladder_steps_below_it_and_back_to_it() {
        let mut g = Governor::new(25, true, secs(0.0));
        let _ = run(&mut g, 0.0, 6.0, true, true);
        assert_eq!(g.target(), 20);
        let _ = run(&mut g, 6.0, 125.0, false, true);
        assert_eq!(g.target(), 25);
    }

    #[test]
    fn off_it_never_moves_and_a_reset_restores() {
        let mut g = Governor::new(60, false, secs(0.0));
        assert!(run(&mut g, 0.0, 10.0, true, true).is_empty());
        let mut g = Governor::new(60, true, secs(0.0));
        let _ = run(&mut g, 0.0, 6.0, true, true);
        assert_eq!(g.target(), 45);
        g.reset(secs(6.0));
        assert_eq!(g.target(), 60);
        assert!(
            run(&mut g, 6.0, 2.5, true, true).is_empty(),
            "judged afresh after a reset"
        );
    }
}
