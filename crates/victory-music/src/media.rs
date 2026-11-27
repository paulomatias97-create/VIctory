//! The player's music app through Windows' media controls (the panel the
//! volume keys show): which app is playing, what it plays, its cover, and
//! play, pause and skip. Any app that shows there works (Spotify, YouTube
//! Music, Deezer, Apple Music, Amazon Music, a browser tab), with no
//! account or key of ours.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::core::Result;

/// Apps taken before the system's own pick of the current session: music
/// first, a browser tab (often a video) last.
const MUSIC_APPS: [&str; 8] = [
    "spotify",
    "deezer",
    "applemusic",
    "zunemusic",
    "amazonmusic",
    "tidal",
    "youtube",
    "soundcloud",
];

/// What the music app is playing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NowPlaying {
    /// The app's id as Windows knows it (`Spotify.exe`,
    /// `SpotifyAB.SpotifyMusic_...!Spotify`, `Chrome`, ...).
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// The cover as the app hands it (usually JPEG or PNG), when it has one.
    pub cover: Option<Vec<u8>>,
}

pub struct Media {
    manager: Manager,
}

impl Media {
    /// Connects to the system's media controls. Blocks briefly.
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: Manager::RequestAsync()?.join()?,
        })
    }

    /// The session to drive: a known music app if one is open, else the
    /// one the system shows.
    fn session(&self) -> Option<Session> {
        let sessions = self.manager.GetSessions().ok()?;
        let all: Vec<Session> = (0..sessions.Size().ok()?)
            .filter_map(|i| sessions.GetAt(i).ok())
            .collect();
        let rank = |s: &Session| {
            let id = s
                .SourceAppUserModelId()
                .map(|h| h.to_string().to_lowercase());
            id.ok()
                .and_then(|id| MUSIC_APPS.iter().position(|app| id.contains(app)))
        };
        all.iter()
            .filter_map(|s| rank(s).map(|r| (r, s)))
            .min_by_key(|&(r, _)| r)
            .map(|(_, s)| s.clone())
            .or_else(|| self.manager.GetCurrentSession().ok())
    }

    /// What is playing, or `None` with no music app open.
    pub fn now_playing(&self, with_cover: bool) -> Option<NowPlaying> {
        let s = self.session()?;
        let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let text = |r: Result<windows::core::HSTRING>| r.map(|h| h.to_string()).unwrap_or_default();
        let playing = s
            .GetPlaybackInfo()
            .and_then(|i| i.PlaybackStatus())
            .is_ok_and(|st| st == Status::Playing);
        let cover = if with_cover {
            props.Thumbnail().ok().and_then(|t| read_all(&t).ok())
        } else {
            None
        };
        Some(NowPlaying {
            app: text(s.SourceAppUserModelId()),
            title: text(props.Title()),
            artist: text(props.Artist()),
            album: text(props.AlbumTitle()),
            playing,
            cover,
        })
    }

    /// Sends `command` to the music app; `false` if there is none or it
    /// refused.
    pub fn command(&self, command: Command) -> bool {
        let Some(s) = self.session() else {
            return false;
        };
        let op = match command {
            Command::Play => s.TryPlayAsync(),
            Command::Pause => s.TryPauseAsync(),
            Command::Toggle => s.TryTogglePlayPauseAsync(),
            Command::Next => s.TrySkipNextAsync(),
            Command::Previous => s.TrySkipPreviousAsync(),
        };
        op.and_then(|op| op.join()).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// Covers past this are not read (a cover is tens to hundreds of KiB).
const MAX_COVER: u64 = 8 << 20;

fn read_all(r: &windows::Storage::Streams::IRandomAccessStreamReference) -> Result<Vec<u8>> {
    let stream = r.OpenReadAsync()?.join()?;
    let size = stream.Size()?.min(MAX_COVER) as u32;
    let buffer = Buffer::Create(size)?;
    let read = stream
        .ReadAsync(&buffer, size, InputStreamOptions::None)?
        .join()?;
    let reader = DataReader::FromBuffer(&read)?;
    let mut bytes = vec![0u8; read.Length()? as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}
