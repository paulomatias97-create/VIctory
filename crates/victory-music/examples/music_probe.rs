//! Tries the phone's Spotify app without the title, on Windows (Spotify
//! open and playing):
//!
//! ```text
//! cargo run -p victory-music --example music_probe -- now
//! cargo run -p victory-music --example music_probe -- next | previous | toggle
//! cargo run -p victory-music --example music_probe -- scene
//! cargo run -p victory-music --example music_probe -- around
//! ```
//!
//! `around` circles the music around the player (left, ahead, right,
//! behind) through Spotify's balance: use headphones.
//!
//! `scene` plays a scripted half minute against the real Spotify: the radio
//! on in a vehicle, a line of dialogue, walking away, coming back, a pause.
//! The app's own volume is put back at the end; stopping it early with
//! Ctrl+C leaves the level it had then (set it back in the volume mixer).

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};
    use victory_music::media::{Command, Media};
    use victory_music::mix::{Mixer, Scene, Settings};
    use victory_music::volume::AppVolume;

    let what = std::env::args().nth(1).unwrap_or_else(|| "now".into());
    let media = Media::new().expect("Windows media controls");
    let show = |media: &Media| match media.now_playing(true) {
        Some(n) => println!(
            "{} | {} - {} ({}) {} | cover: {}",
            n.app,
            n.artist,
            n.title,
            n.album,
            if n.playing { "playing" } else { "paused" },
            n.cover
                .map_or("none".into(), |c| format!("{} bytes", c.len()))
        ),
        None => println!("Spotify is not open (or has not played anything yet)"),
    };
    let command = match what.as_str() {
        "next" => Some(Command::Next),
        "previous" => Some(Command::Previous),
        "toggle" => Some(Command::Toggle),
        "play" => Some(Command::Play),
        "pause" => Some(Command::Pause),
        _ => None,
    };
    if let Some(c) = command {
        println!(
            "{c:?}: {}",
            if media.command(c) { "done" } else { "refused" }
        );
        std::thread::sleep(Duration::from_millis(500));
        show(&media);
        return;
    }
    show(&media);
    if what == "around" {
        around(&media);
        return;
    }
    if what != "scene" {
        return;
    }
    let Some(app) = media.now_playing(false).map(|n| n.app) else {
        return;
    };
    let car = Scene::default();
    let steps: Vec<(&str, f32, Scene)> = vec![
        ("in the car, radio on", 6.0, car),
        (
            "dialogue",
            5.0,
            Scene {
                dialogue: true,
                ..car
            },
        ),
        ("dialogue over", 4.0, car),
        (
            "out of the car",
            3.0,
            Scene {
                in_vehicle: false,
                ..car
            },
        ),
        (
            "walking away: 10 m",
            3.0,
            Scene {
                in_vehicle: false,
                distance: 10.0,
                ..car
            },
        ),
        (
            "walking away: 20 m",
            3.0,
            Scene {
                in_vehicle: false,
                distance: 20.0,
                ..car
            },
        ),
        ("back in the car", 5.0, car),
        (
            "pause menu",
            3.0,
            Scene {
                paused: true,
                ..car
            },
        ),
        ("back to the game", 4.0, car),
    ];
    let mut volume = AppVolume::new();
    let mut mixer = Mixer::new(Settings::default());
    let dt = 1.0 / 30.0;
    for (label, seconds, scene) in steps {
        println!("{label}");
        let end = Instant::now() + Duration::from_secs_f32(seconds);
        while Instant::now() < end {
            if let Some(level) = mixer.step(&scene, dt) {
                let n = volume.set(&app, level);
                if n == 0 {
                    println!("  (Spotify's volume could not be found in the mixer)");
                }
            }
            std::thread::sleep(Duration::from_secs_f32(dt));
        }
        println!("  level {:.2}", mixer.level());
    }
    volume.restore(&app);
    println!("volume put back");
}

/// The vehicle with the music, 6 m away, circling the player once in 16
/// seconds: left, ahead, right, behind, left. Headphones make it plain.
#[cfg(windows)]
fn around(media: &victory_music::media::Media) {
    use std::time::{Duration, Instant};
    use victory_music::mix::{Mixer, Scene, Settings};
    use victory_music::space::Panner;
    use victory_music::volume::AppVolume;

    let Some(app) = media.now_playing(false).map(|n| n.app) else {
        return;
    };
    let mut volume = AppVolume::new();
    let mut mixer = Mixer::new(Settings::default());
    let mut panner = Panner::new();
    let dt = 1.0 / 30.0;
    let start = Instant::now();
    let total = 16.0f32;
    let mut said = String::new();
    while start.elapsed().as_secs_f32() < total {
        let t = start.elapsed().as_secs_f32() / total;
        // From the left (-90 degrees), clockwise round to the left again.
        let bearing = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::TAU;
        let scene = Scene {
            in_vehicle: false,
            distance: 6.0,
            bearing,
            ..Scene::default()
        };
        if let Some(level) = mixer.step(&scene, dt) {
            volume.set(&app, level);
        }
        if let Some((l, r)) = panner.step(&scene, dt) {
            volume.set_balance(&app, l, r);
        }
        let deg = (bearing.to_degrees() + 540.0) % 360.0 - 180.0;
        let side = match deg {
            d if (-135.0..-45.0).contains(&d) => "left",
            d if (-45.0..45.0).contains(&d) => "ahead",
            d if (45.0..135.0).contains(&d) => "right",
            _ => "behind",
        };
        if side != said {
            let (l, r) = panner.gains();
            println!("{side:>6}  left {l:.2}  right {r:.2}");
            said = side.into();
        }
        std::thread::sleep(Duration::from_secs_f32(dt));
    }
    volume.restore(&app);
    println!("volume and balance put back");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("music_probe needs Windows (media controls and the volume mixer)");
}
