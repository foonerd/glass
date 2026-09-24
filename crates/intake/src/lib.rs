//! Poll the outside world and return the latest [`lead::Input`].
//! This station does not parse skin geometry or draw.

use lead::Input;

/// A source of snapshots. The player polls a FIFO; the remote polls UDP.
pub trait Source {
    fn poll(&mut self) -> Input;
}

/// Placeholder source used until a real FIFO or socket is wired.
#[derive(Debug, Default)]
pub struct IdleSource;

impl Source for IdleSource {
    fn poll(&mut self) -> Input {
        Input::default()
    }
}
