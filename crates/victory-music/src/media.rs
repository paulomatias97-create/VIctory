//! The player's Spotify app through Windows' media controls (the panel the
//! volume keys show): what it plays, its cover, and play, pause and skip.
//! Both the desktop app and the Microsoft Store one show there, with no
//! account or key of ours. Other apps are left alone, even when they are
//! the ones the system shows.

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::core::Result;

use crate::apps::is_spotify;

/// What Spotify is playing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NowPlaying {
    /// The app's id as Windows knows it (`Spotify.exe`, or
    /// `SpotifyAB.SpotifyMusic_...!Spotify` from the Store).
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

    /// Spotify's session, if it is open.
    fn session(&self) -> Option<Session> {
        let sessions = self.manager.GetSessions().ok()?;
        (0..sessions.Size().ok()?)
            .filter_map(|i| sessions.GetAt(i).ok())
            .find(|s| {
                s.SourceAppUserModelId()
                    .is_ok_and(|id| is_spotify(&id.to_string()))
            })
    }

    /// What Spotify is playing, or `None` when it is not open.
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

    /// Sends `command` to Spotify; `false` if it is not open or refused.
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
