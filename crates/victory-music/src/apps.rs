//! Telling Spotify apart, and which process is the app the media controls
//! name.

/// Whether a media app id is Spotify's (desktop `Spotify.exe`, or the
/// Store's `SpotifyAB.SpotifyMusic_...!Spotify`).
pub fn is_spotify(app: &str) -> bool {
    app.to_lowercase().contains("spotify")
}

/// The key an app id and a process name are compared by: lower case, the
/// part after `!` of a packaged app's id, without `.exe`.
fn key(name: &str) -> String {
    let name = name.rsplit('!').next().unwrap_or(name);
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let name = name.to_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// Whether the process `exe` is the app whose media id is `app`.
pub fn matches(app: &str, exe: &str) -> bool {
    let (a, e) = (key(app), key(exe));
    !e.is_empty()
        && (a == e || (e.len() >= 4 && a.contains(&e)) || (a.len() >= 4 && e.contains(&a)))
}

#[cfg(test)]
mod tests {
    use super::{is_spotify, matches};

    #[test]
    fn only_spotify_is_spotify() {
        assert!(is_spotify("Spotify.exe"));
        assert!(is_spotify("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"));
        assert!(!is_spotify("Chrome"));
        assert!(!is_spotify(
            "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"
        ));
    }

    #[test]
    fn media_ids_match_their_processes() {
        assert!(matches(
            "Spotify.exe",
            r"C:\Users\a\AppData\Roaming\Spotify\Spotify.exe"
        ));
        assert!(matches(
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            r"C:\Program Files\WindowsApps\SpotifyAB...\Spotify.exe"
        ));
        assert!(matches(
            "Chrome",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        assert!(matches("MSEdge", r"C:\...\msedge.exe"));
        assert!(!matches("Spotify.exe", r"C:\Games\victory.exe"));
        assert!(!matches("Chrome", r"C:\x\a.exe"));
    }
}
