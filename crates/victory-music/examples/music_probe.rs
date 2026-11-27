//! Tries the music app without the title, on Windows:
//!
//! ```text
//! cargo run -p victory-music --example music_probe -- now
//! cargo run -p victory-music --example music_probe -- next | previous | toggle
//! cargo run -p victory-music --example music_probe -- scene
//! ```
//!
//! `scene` plays a scripted minute against the real music app: the radio
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
        None => println!("no music app open: start Spotify, YouTube Music, Deezer..."),
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
                    println!("  (the app's volume could not be found in the mixer)");
                }
            }
            std::thread::sleep(Duration::from_secs_f32(dt));
        }
        println!("  level {:.2}", mixer.level());
    }
    volume.restore(&app);
    println!("volume put back");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("music_probe needs Windows (media controls and the volume mixer)");
}
