//! The phone running beside the title (Windows): a thread that reads the
//! phone's keys, asks Spotify what it plays, sets Spotify's volume from the
//! scene, and keeps the state the overlay is drawn from. [`Overlay`] draws
//! it on demand, for the presentation's overlay.
//!
//! Keys, only while one of our process's windows has the focus:
//! F7 opens and closes the phone, F8 plays or pauses, F9 skips forward,
//! F6 back.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F6, VK_F7, VK_F8, VK_F9};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::media::{Command, Media};
use crate::mix::{Mixer, Scene, Settings, label};
use crate::space::Panner;
use crate::ui::{Fonts, Phone, PhoneState, Track};
use crate::volume::AppVolume;

/// How often the thread runs, and asks Spotify what it plays.
const TICK: Duration = Duration::from_millis(33);
const ASK: Duration = Duration::from_millis(500);
/// Seconds to slide the phone fully in or out.
const SLIDE: f32 = 0.28;

struct Shared {
    state: PhoneState,
    /// Bumped whenever what the phone shows changes.
    serial: u64,
}

/// A drawn phone: premultiplied RGBA pixels, the part that may have
/// changed (x, y, width, height), and a serial that changes with them.
pub struct Frame {
    pub pixels: Vec<u8>,
    pub changed: (u32, u32, u32, u32),
    pub serial: u64,
}

/// What [`Overlay::picture`] has for the caller.
pub enum Drawn {
    /// The phone is hidden.
    Hidden,
    /// Nothing changed since the frame with the serial the caller has.
    Same,
    New(Frame),
}

/// The phone's picture, drawn from the thread's latest state.
pub struct Overlay {
    shared: Arc<Mutex<Shared>>,
    phone: Mutex<Phone>,
}

impl Overlay {
    /// The phone over a `width` by `height` output, unless the caller's
    /// frame (`known`, a serial) still stands.
    pub fn picture(&self, width: u32, height: u32, known: Option<u64>) -> Drawn {
        let (state, serial) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.state.clone(), s.serial)
        };
        if state.open <= 0.0 {
            return Drawn::Hidden;
        }
        if known == Some(serial) {
            return Drawn::Same;
        }
        let mut phone = self.phone.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pix) = phone.render(&state, width, height) else {
            return Drawn::Hidden;
        };
        // The phone and its shadow keep to the right part of the picture.
        let scale = height as f32 / 1080.0;
        let left = (width as f32 - (crate::ui::phone::W + 48.0 + 60.0) * scale).max(0.0) as u32;
        Drawn::New(Frame {
            pixels: pix.take(),
            changed: (left, 0, width - left, height),
            serial,
        })
    }
}

/// Where the scene comes from: the title (through its adapter), the
/// simulator, or nothing yet (the music as loud as the player left it).
pub type SceneSource = Box<dyn FnMut() -> Scene + Send>;

/// The scene when nothing reads it: in a vehicle, nothing going on.
pub fn no_scene() -> SceneSource {
    Box::new(Scene::default)
}

/// Starts the phone's thread. `fonts_dir`: where Rubik and Inter are (else
/// `VICTORY_FONTS_DIR`, else the system's fonts); `scene`: asked every tick.
pub fn start(
    fonts_dir: Option<&std::path::Path>,
    scene: SceneSource,
) -> Result<Arc<Overlay>, String> {
    let fonts = Fonts::load(fonts_dir)?;
    let shared = Arc::new(Mutex::new(Shared {
        state: PhoneState {
            scene: "Livre".into(),
            level: 1.0,
            ..PhoneState::default()
        },
        serial: 0,
    }));
    let overlay = Arc::new(Overlay {
        shared: shared.clone(),
        phone: Mutex::new(Phone::new(fonts)),
    });
    std::thread::Builder::new()
        .name("victory-phone".into())
        .spawn(move || run(&shared, scene))
        .map_err(|e| e.to_string())?;
    Ok(overlay)
}

fn run(shared: &Mutex<Shared>, mut source: SceneSource) {
    let media = match Media::new() {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::warn!(error = %e, "phone: no media controls; Spotify cannot be driven");
            None
        }
    };
    let mut volume = AppVolume::new();
    let mut mixer = Mixer::new(Settings::default());
    let mut panner = Panner::new();
    let mut keys = Keys::default();
    let started = Instant::now();
    let mut last = Instant::now();
    let mut asked: Option<Instant> = None;
    let mut track: Option<Track> = None;
    let mut app = String::new();
    let mut open_target = false;
    let mut open = 0.0f32;
    let mut opened_at = 0.0f32;
    loop {
        std::thread::sleep(TICK);
        let now = Instant::now();
        let dt = (now - last).as_secs_f32();
        last = now;

        let pressed = keys.read();
        if pressed.toggle {
            open_target = !open_target;
            if open_target {
                opened_at = started.elapsed().as_secs_f32();
            }
        }
        if let Some(m) = &media {
            let command = if pressed.play {
                Some(Command::Toggle)
            } else if pressed.next {
                Some(Command::Next)
            } else if pressed.previous {
                Some(Command::Previous)
            } else {
                None
            };
            if let Some(c) = command {
                m.command(c);
                asked = None; // Ask again at once.
            }
        }

        // What Spotify plays: the cover only when the song changes.
        if asked.is_none_or(|t| now - t >= ASK) {
            asked = Some(now);
            if let Some(m) = &media {
                let np = m.now_playing(false);
                track = match np {
                    None => None,
                    Some(n) => {
                        app = n.app.clone();
                        let same = track
                            .as_ref()
                            .is_some_and(|t| n.title == t.title && n.artist == t.artist);
                        let cover = if same {
                            track.as_ref().and_then(|t| t.cover.clone())
                        } else {
                            m.now_playing(true).and_then(|n| n.cover)
                        };
                        Some(Track {
                            title: n.title,
                            artist: n.artist,
                            playing: n.playing,
                            cover,
                        })
                    }
                };
            }
        }

        // Spotify's volume and balance from the scene.
        let scene = source();
        if let Some(level) = mixer.step(&scene, dt)
            && !app.is_empty()
        {
            volume.set(&app, level);
        }
        // Where the music comes from.
        if let Some((left, right)) = panner.step(&scene, dt)
            && !app.is_empty()
        {
            volume.set_balance(&app, left, right);
        }

        // Slide.
        let goal = if open_target { 1.0 } else { 0.0 };
        let step = dt / SLIDE;
        open = if open < goal {
            (open + step).min(goal)
        } else {
            (open - step).max(goal)
        };

        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let state = PhoneState {
            open,
            clock: clock(),
            track: track.clone(),
            level: mixer.level(),
            scene: label(&scene),
            time: started.elapsed().as_secs_f32() - opened_at,
        };
        let moving = open > 0.0 && state.track.as_ref().is_some_and(|t| t.playing);
        let changed = moving
            || state.open != s.state.open
            || state.track != s.state.track
            || state.clock != s.state.clock
            || (state.level - s.state.level).abs() > 0.005;
        s.state = state;
        if changed {
            s.serial += 1;
        }
    }
}

fn clock() -> String {
    // SAFETY: no arguments; returns the local time.
    let t = unsafe { GetLocalTime() };
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

#[derive(Default)]
struct Keys {
    down: [bool; 4],
}

#[derive(Default)]
struct Pressed {
    toggle: bool,
    play: bool,
    next: bool,
    previous: bool,
}

impl Keys {
    /// Keys that went down since the last read, while our window has the
    /// focus.
    fn read(&mut self) -> Pressed {
        let ours = focused();
        let codes = [VK_F7, VK_F8, VK_F9, VK_F6];
        let mut went = [false; 4];
        for (i, vk) in codes.iter().enumerate() {
            // SAFETY: a plain query of the key's state.
            let down = ours && unsafe { GetAsyncKeyState(i32::from(vk.0)) } as u16 & 0x8000 != 0;
            went[i] = down && !self.down[i];
            self.down[i] = down;
        }
        Pressed {
            toggle: went[0],
            play: went[1],
            next: went[2],
            previous: went[3],
        }
    }
}

/// Whether the window with the focus is one of our process's.
fn focused() -> bool {
    // SAFETY: plain queries; a null window gives process 0.
    unsafe {
        let window = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        pid == std::process::id()
    }
}
