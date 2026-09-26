//! The audio tap's measurements and the ring they travel through.
//!
//! The tap runs inside the audio player (the `glasstap` library, an ALSA
//! PCM plugin the stream passes through untouched, and an ALSA scope for
//! the `meter` PCM) and measures the stream a hop at a time: the peak and
//! RMS of each channel and the magnitude spectrum of each channel, or for
//! one-bit audio the density level. Every hop it publishes one [`Frame`]
//! into a shared ring under `/dev/shm`, one file per writer, which the
//! display reads at its own frame rate. The audio thread only copies and
//! hands samples across the `relay`; the measuring happens on a thread of
//! its own. Nothing here blocks the audio, and nothing here allocates once
//! the stream is set up.
//!
//! The `legacy` module writes the same two FIFOs the previous tap wrote, so
//! the two can run side by side while the switch is made.

pub mod analysis;
pub mod dop;
pub mod dsd;
pub mod fifo;
pub mod legacy;
pub mod measure;
pub mod relay;
pub mod ring;
pub mod sample;
pub mod wire;

pub use analysis::Analyser;
pub use ring::{Frame, Reader, Writer, MAX_CHANNELS};
