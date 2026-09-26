//! The audio tap's measurements and the ring they travel through.
//!
//! The tap runs inside the audio player as an ALSA scope (the `glasstap`
//! library) and measures the stream a hop at a time: the peak and RMS of
//! each channel and the magnitude spectrum of each channel. Every hop it
//! publishes one [`Frame`] into a shared ring under `/dev/shm`, one file per
//! writing process, which the display reads at its own frame rate. Nothing
//! here blocks, and nothing here allocates once the stream is set up.
//!
//! The `legacy` module writes the same two FIFOs the previous tap wrote, so
//! the two can run side by side while the switch is made.

pub mod analysis;
pub mod dop;
pub mod fifo;
pub mod legacy;
pub mod ring;

pub use analysis::Analyser;
pub use ring::{Frame, Reader, Writer, MAX_CHANNELS};
