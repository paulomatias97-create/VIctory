//! The phone's music app, on Spotify. The music is the player's own
//! Spotify: Windows' media controls give what it plays and take play, pause
//! and skip; the scene sets its volume in the system mixer. No audio passes
//! through us, so Spotify's audio is never captured or altered, only made
//! louder or quieter.
//!
//! * [`mix`]: the volume the scene calls for, gliding between levels;
//! * [`apps`]: telling Spotify's media session and process apart;
//! * `media` (Windows): what Spotify plays, its cover, and play/pause/skip;
//! * `volume` (Windows): Spotify's own volume in the system mixer.

pub mod apps;
pub mod mix;

#[cfg(windows)]
pub mod media;
#[cfg(windows)]
pub mod volume;
