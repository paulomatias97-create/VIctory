//! The phone's music app. The music is the player's own: whatever their
//! music app plays (Spotify, YouTube Music, Deezer, Apple Music, a
//! browser), found and driven through Windows' media controls, with its
//! volume following the scene through Windows' per-app volume. No audio
//! passes through us, so streamed music is never captured or altered,
//! only made louder or quieter.
//!
//! * [`mix`]: the volume the scene calls for, gliding between levels;
//! * `media` (Windows): what is playing, its cover, and play/pause/skip;
//! * `volume` (Windows): the music app's own volume in the system mixer.

pub mod apps;
pub mod mix;

#[cfg(windows)]
pub mod media;
#[cfg(windows)]
pub mod volume;
