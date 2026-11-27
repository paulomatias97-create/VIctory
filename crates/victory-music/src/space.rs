//! Where the music comes from: outside the vehicle, its radio is heard
//! from the vehicle's side (left or right of the camera), a little dimmer
//! from behind. Applied as the left and right channel levels of the music
//! app in the system mixer (the mixer's own balance), so the audio itself
//! is never touched: no front/back cues or muffling, only the balance a
//! stereo pair can give. Inside the vehicle the music is all around: both
//! sides full.

use crate::mix::Scene;

/// How much quieter a source straight behind is.
const BEHIND: f32 = 0.25;
/// How fast the balance follows the camera (a full left-to-right swing
/// takes 1 / this seconds).
const SWING: f32 = 2.5;
/// Changes below this are not worth a call to the system mixer.
const STEP: f32 = 0.01;

/// The (left, right) levels for `pan` (-1 left to 1 right) and `back` (0
/// ahead to 1 straight behind): an equal-power pan, scaled so a source
/// ahead keeps both sides full.
pub fn gains(pan: f32, back: f32) -> (f32, f32) {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    let k = std::f32::consts::SQRT_2 * (1.0 - BEHIND * back.clamp(0.0, 1.0));
    ((theta.cos() * k).min(1.0), (theta.sin() * k).min(1.0))
}

/// Where `scene` puts the music: (pan, back).
pub fn place(scene: &Scene) -> (f32, f32) {
    if scene.in_vehicle {
        return (0.0, 0.0);
    }
    let (sin, cos) = scene.bearing.sin_cos();
    (sin, (-cos).max(0.0))
}

/// The balance as it follows the scene.
#[derive(Debug, Clone, Default)]
pub struct Panner {
    pan: f32,
    back: f32,
    applied: Option<(f32, f32)>,
}

impl Panner {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current (left, right) levels.
    pub fn gains(&self) -> (f32, f32) {
        gains(self.pan, self.back)
    }

    /// Advances `dt` seconds towards `scene`. Returns the (left, right)
    /// levels when they moved enough to apply.
    pub fn step(&mut self, scene: &Scene, dt: f32) -> Option<(f32, f32)> {
        let (pan, back) = place(scene);
        let most = SWING * 2.0 * dt;
        self.pan += (pan - self.pan).clamp(-most, most);
        self.back += (back - self.back).clamp(-most, most);
        let now = self.gains();
        let settled =
            (pan - self.pan).abs() < f32::EPSILON && (back - self.back).abs() < f32::EPSILON;
        match self.applied {
            Some(a)
                if (a.0 - now.0).abs() < STEP
                    && (a.1 - now.1).abs() < STEP
                    && !(settled && a != now) =>
            {
                None
            }
            _ => {
                self.applied = Some(now);
                Some(now)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn ahead_is_full_sides_lean_behind_is_dimmer() {
        assert!(close(gains(0.0, 0.0), (1.0, 1.0)));
        let right = gains(1.0, 0.0);
        assert!(right.0 < 0.01 && right.1 > 0.99, "{right:?}");
        let left = gains(-1.0, 0.0);
        assert!(left.0 > 0.99 && left.1 < 0.01, "{left:?}");
        let behind = gains(0.0, 1.0);
        assert!(close(behind, (0.75, 0.75)), "{behind:?}");
    }

    #[test]
    fn the_scene_places_the_vehicle() {
        let car = Scene {
            in_vehicle: false,
            distance: 5.0,
            ..Scene::default()
        };
        let at = |b: f32| place(&Scene { bearing: b, ..car });
        use std::f32::consts::{FRAC_PI_2, PI};
        assert!((at(FRAC_PI_2).0 - 1.0).abs() < 1e-6);
        assert!((at(-FRAC_PI_2).0 + 1.0).abs() < 1e-6);
        assert!((at(PI).1 - 1.0).abs() < 1e-6);
        // Inside it, all around.
        assert_eq!(
            place(&Scene {
                bearing: 1.0,
                ..Scene::default()
            }),
            (0.0, 0.0)
        );
    }

    #[test]
    fn the_balance_swings_smoothly() {
        let mut p = Panner::new();
        let right = Scene {
            in_vehicle: false,
            bearing: std::f32::consts::FRAC_PI_2,
            ..Scene::default()
        };
        p.step(&right, 0.05);
        let partway = p.gains();
        assert!(partway.0 > 0.5 && partway.1 >= partway.0, "{partway:?}");
        for _ in 0..30 {
            p.step(&right, 1.0 / 30.0);
        }
        let there = p.gains();
        assert!(there.0 < 0.01 && there.1 > 0.99, "{there:?}");
        assert_eq!(p.step(&right, 1.0 / 30.0), None);
    }
}
