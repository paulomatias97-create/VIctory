//! The music's volume as the scene changes: full in a vehicle with the
//! radio on, down under dialogue, fading with distance when the player
//! walks away from the vehicle, silent while the title is paused. Changes
//! glide (a fade, not a jump), faster going down for dialogue than coming
//! back.
//!
//! Nothing here touches the audio itself: the level is applied as the
//! volume of the player's Spotify (see `volume`), so its music is
//! only ever made louder or quieter.

/// What the title is doing, as far as the music cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scene {
    /// The player has the music on (the phone app or a vehicle radio).
    pub music_on: bool,
    /// The player is in the vehicle whose radio plays the music.
    pub in_vehicle: bool,
    /// Someone is speaking (dialogue, a phone call, a cutscene line).
    pub dialogue: bool,
    /// The title is paused or in a menu.
    pub paused: bool,
    /// Metres from the player to the vehicle playing the music (ignored in
    /// it).
    pub distance: f32,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            music_on: true,
            in_vehicle: true,
            dialogue: false,
            paused: false,
            distance: 0.0,
        }
    }
}

/// How the volume follows the scene (gains are 0 to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The level in a vehicle with nothing else going on.
    pub master: f32,
    /// The gain under dialogue.
    pub dialogue: f32,
    /// The gain just outside the vehicle (doors between the player and the
    /// speakers).
    pub outside: f32,
    /// Metres from the vehicle at which the music can no longer be heard.
    pub hearing: f32,
    /// Seconds to fade in (music turned on, back in the vehicle).
    pub fade_in: f32,
    /// Seconds to fade out (music turned off, pause).
    pub fade_out: f32,
    /// Seconds to go down for dialogue.
    pub duck: f32,
    /// Seconds to come back after dialogue.
    pub unduck: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master: 1.0,
            dialogue: 0.3,
            outside: 0.55,
            hearing: 25.0,
            fade_in: 1.2,
            fade_out: 0.6,
            duck: 0.25,
            unduck: 1.0,
        }
    }
}

/// The level the music settles at in `scene`.
pub fn target(s: &Settings, scene: &Scene) -> f32 {
    if !scene.music_on || scene.paused {
        return 0.0;
    }
    let mut g = s.master;
    if !scene.in_vehicle {
        // Quieter through the doors, then gone over the hearing distance
        // (a squared falloff sounds closer to distance than a straight one).
        let left = (1.0 - scene.distance.max(0.0) / s.hearing.max(0.01)).clamp(0.0, 1.0);
        g *= s.outside * left * left;
    }
    if scene.dialogue {
        g *= s.dialogue;
    }
    g.clamp(0.0, 1.0)
}

/// The level as it glides towards the scene's target.
#[derive(Debug, Clone)]
pub struct Mixer {
    pub settings: Settings,
    level: f32,
    applied: Option<f32>,
}

/// Steps below this are not worth a call to the system mixer.
const STEP: f32 = 0.005;

impl Mixer {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            level: 0.0,
            applied: None,
        }
    }

    /// The current level.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Advances `dt` seconds towards `scene`'s target. Returns the level
    /// when it moved enough to apply, `None` when the last applied one
    /// still stands.
    pub fn step(&mut self, scene: &Scene, dt: f32) -> Option<f32> {
        let s = &self.settings;
        let goal = target(s, scene);
        let seconds = if goal < self.level {
            if scene.dialogue && scene.music_on && !scene.paused {
                s.duck
            } else {
                s.fade_out
            }
        } else if self.level < s.dialogue * s.master && scene.music_on && !scene.dialogue {
            // Coming up from (near) silence: a fade in.
            s.fade_in
        } else {
            s.unduck
        };
        // A straight glide covering the whole range in `seconds`, so short
        // changes take proportionally less time.
        let most = if seconds > 0.0 { dt / seconds } else { 1.0 };
        let delta = (goal - self.level).clamp(-most, most);
        self.level = (self.level + delta).clamp(0.0, 1.0);
        let settled = (self.level - goal).abs() < f32::EPSILON;
        match self.applied {
            Some(a) if (a - self.level).abs() < STEP && !(settled && a != self.level) => None,
            _ => {
                self.applied = Some(self.level);
                Some(self.level)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(m: &mut Mixer, scene: Scene, seconds: f32) -> f32 {
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt).round() as u32 {
            m.step(&scene, dt);
        }
        m.level()
    }

    #[test]
    fn targets_follow_the_scene() {
        let s = Settings::default();
        let car = Scene::default();
        assert_eq!(target(&s, &car), 1.0);
        assert_eq!(
            target(
                &s,
                &Scene {
                    dialogue: true,
                    ..car
                }
            ),
            0.3
        );
        assert_eq!(
            target(
                &s,
                &Scene {
                    paused: true,
                    ..car
                }
            ),
            0.0
        );
        assert_eq!(
            target(
                &s,
                &Scene {
                    music_on: false,
                    ..car
                }
            ),
            0.0
        );
        let out = Scene {
            in_vehicle: false,
            ..car
        };
        assert!((target(&s, &out) - 0.55).abs() < 1e-6);
        let far = target(
            &s,
            &Scene {
                distance: 12.5,
                ..out
            },
        );
        assert!((far - 0.55 * 0.25).abs() < 1e-6);
        assert_eq!(
            target(
                &s,
                &Scene {
                    distance: 40.0,
                    ..out
                }
            ),
            0.0
        );
    }

    #[test]
    fn fades_in_and_ducks_faster_than_it_comes_back() {
        let mut m = Mixer::new(Settings::default());
        let car = Scene::default();
        // Fading in takes about the fade-in time, not a frame.
        assert!(run(&mut m, car, 0.3) < 0.5);
        assert_eq!(run(&mut m, car, 1.5), 1.0);
        // Dialogue: down within the duck time.
        let talk = Scene {
            dialogue: true,
            ..car
        };
        assert!((run(&mut m, talk, 0.3) - 0.3).abs() < 1e-3);
        // Back up more slowly than it went down: in the same 0.3 s it went
        // down 0.7, it comes up about 0.3.
        let back = run(&mut m, car, 0.3);
        assert!(back - 0.3 < 0.35, "{back}");
        assert_eq!(run(&mut m, car, 1.0), 1.0);
    }

    #[test]
    fn walking_away_fades_to_silence() {
        let mut m = Mixer::new(Settings::default());
        let car = Scene::default();
        run(&mut m, car, 2.0);
        let mut levels = Vec::new();
        for d in [0.0f32, 5.0, 10.0, 20.0, 30.0] {
            let out = Scene {
                in_vehicle: false,
                distance: d,
                ..car
            };
            levels.push(run(&mut m, out, 2.0));
        }
        assert!(levels.windows(2).all(|w| w[1] < w[0]), "{levels:?}");
        assert_eq!(*levels.last().unwrap(), 0.0);
    }

    #[test]
    fn only_changes_worth_applying_are_reported() {
        let mut m = Mixer::new(Settings::default());
        let car = Scene::default();
        let dt = 1.0 / 60.0;
        let mut calls = 0;
        for _ in 0..600 {
            calls += usize::from(m.step(&car, dt).is_some());
        }
        // A fade over 1.2 s in steps of at least 0.005, then nothing.
        assert!(calls <= 201, "{calls}");
        assert_eq!(m.step(&car, dt), None);
        // The final level was applied exactly.
        assert_eq!(m.applied, Some(1.0));
    }
}
