//! A stand-in for the title, to try the whole phone without it (Windows):
//! a city seen from above with a car and the player. Drive, get out, walk
//! around the car, take a call, pause: the scene the title will give the
//! phone comes from here, so Spotify's volume and balance and the phone on
//! screen behave as they will in the game.
//!
//! ```text
//! cargo run --release -p victory-music --example leonida_sim
//! ```
//!
//! | Keys | |
//! |------|--|
//! | W A S D | walk / drive and turn |
//! | E | get in or out of the car (close to it) |
//! | T | a phone call starts or ends (dialogue) |
//! | P | pause menu |
//! | F7 | opens the phone; F8 play/pause, F9 next, F6 previous |
//!
//! Headphones make the side the car is on plain.

#[cfg(windows)]
fn main() {
    sim::run();
}

/// Elsewhere (no Spotify, no window): a few moments drawn to PNG files, to
/// look at the city and the readings.
#[cfg(not(windows))]
fn main() {
    sim::snapshots(std::path::Path::new(
        &std::env::args().nth(1).unwrap_or_else(|| ".".into()),
    ));
}

mod sim {
    use std::f32::consts::PI;
    #[cfg(windows)]
    use std::f32::consts::TAU;
    #[cfg(windows)]
    use std::sync::{Arc, Mutex};
    #[cfg(windows)]
    use std::time::Instant;

    #[cfg(windows)]
    use minifb::{Key, KeyRepeat, Window, WindowOptions};
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
    #[cfg(windows)]
    use tiny_skia::{IntSize, PixmapPaint};
    use victory_music::mix::{Scene, label};
    #[cfg(windows)]
    use victory_music::service::{Drawn, start};
    use victory_music::space::{gains, place};
    use victory_music::ui::{Fonts, draw};

    const W: usize = 1280;
    const H: usize = 720;
    /// Pixels per metre.
    const ZOOM: f32 = 9.0;
    /// City blocks: a road every `BLOCK` metres, `ROAD` wide.
    const BLOCK: f32 = 40.0;
    const ROAD: f32 = 12.0;
    const HEARING: f32 = 25.0;

    struct World {
        player: (f32, f32),
        /// Where the player (or the car, inside it) faces: 0 up, clockwise.
        heading: f32,
        car: (f32, f32),
        car_heading: f32,
        /// Metres a second, driving (only the window drives).
        #[cfg_attr(not(windows), allow(dead_code))]
        speed: f32,
        in_car: bool,
        dialogue: bool,
        paused: bool,
    }

    impl World {
        fn scene(&self) -> Scene {
            let (dx, dy) = (self.car.0 - self.player.0, self.car.1 - self.player.1);
            let distance = (dx * dx + dy * dy).sqrt();
            // The car seen from the camera: ahead is along the heading.
            let (s, c) = self.heading.sin_cos();
            let ahead = dx * s - dy * c;
            let right = dx * c + dy * s;
            Scene {
                music_on: true,
                in_vehicle: self.in_car,
                dialogue: self.dialogue,
                paused: self.paused,
                distance,
                bearing: right.atan2(ahead),
            }
        }

        #[cfg(windows)]
        fn step(&mut self, win: &Window, dt: f32) {
            if win.is_key_pressed(Key::P, KeyRepeat::No) {
                self.paused = !self.paused;
            }
            if win.is_key_pressed(Key::T, KeyRepeat::No) {
                self.dialogue = !self.dialogue;
            }
            if self.paused {
                return;
            }
            if win.is_key_pressed(Key::E, KeyRepeat::No) {
                if self.in_car {
                    self.in_car = false;
                    self.speed = 0.0;
                    // Out of the driver's door, on the car's left.
                    let (s, c) = self.car_heading.sin_cos();
                    self.player = (self.car.0 - c * 2.2, self.car.1 - s * 2.2);
                    self.heading = self.car_heading;
                } else if self.scene().distance < 4.0 {
                    self.in_car = true;
                    self.heading = self.car_heading;
                }
            }
            let fwd = f32::from(u8::from(win.is_key_down(Key::W)))
                - f32::from(u8::from(win.is_key_down(Key::S)));
            let turn = f32::from(u8::from(win.is_key_down(Key::D)))
                - f32::from(u8::from(win.is_key_down(Key::A)));
            if self.in_car {
                self.speed += fwd * 14.0 * dt;
                self.speed -= self.speed * 0.8 * dt;
                self.speed = self.speed.clamp(-6.0, 24.0);
                self.car_heading = (self.car_heading
                    + turn * 1.6 * dt * (self.speed / 10.0).clamp(-1.0, 1.0))
                    % TAU;
                let (s, c) = self.car_heading.sin_cos();
                self.car.0 += s * self.speed * dt;
                self.car.1 -= c * self.speed * dt;
                self.player = self.car;
                self.heading = self.car_heading;
            } else {
                self.heading = (self.heading + turn * 2.6 * dt) % TAU;
                let (s, c) = self.heading.sin_cos();
                self.player.0 += s * fwd * 5.0 * dt;
                self.player.1 -= c * fwd * 5.0 * dt;
            }
        }
    }

    fn start_world() -> World {
        World {
            player: (0.0, BLOCK / 2.0),
            heading: 0.0,
            car: (0.0, BLOCK / 2.0),
            car_heading: 0.0,
            speed: 0.0,
            in_car: true,
            dialogue: false,
            paused: false,
        }
    }

    /// A few moments drawn to `dir`: in the car, out with the car on the
    /// left, the car behind, a call, the pause menu.
    #[cfg(not(windows))]
    pub fn snapshots(dir: &std::path::Path) {
        let fonts = Fonts::load(None).expect("fonts");
        let mut frame = Pixmap::new(W as u32, H as u32).unwrap();
        let base = start_world();
        let (cx, cy) = base.car;
        let shots: [(&str, World); 5] = [
            ("1-no-carro", start_world()),
            (
                "2-carro-a-esquerda",
                World {
                    in_car: false,
                    player: (cx + 6.0, cy),
                    heading: 0.0,
                    ..start_world()
                },
            ),
            (
                "3-carro-atras",
                World {
                    in_car: false,
                    player: (cx, cy - 12.0),
                    heading: 0.0,
                    ..start_world()
                },
            ),
            (
                "4-ligacao",
                World {
                    in_car: false,
                    player: (cx - 8.0, cy - 3.0),
                    heading: 1.2,
                    dialogue: true,
                    ..start_world()
                },
            ),
            (
                "5-pausa",
                World {
                    paused: true,
                    ..start_world()
                },
            ),
        ];
        let _ = base;
        for (name, w) in shots {
            paint_world(&mut frame, &w, &fonts);
            hud(&mut frame, &fonts, &w.scene());
            let path = dir.join(format!("{name}.png"));
            frame.save_png(&path).expect("save");
            println!("{}: {}", path.display(), label(&w.scene()));
        }
    }

    #[cfg(windows)]
    pub fn run() {
        let world = Arc::new(Mutex::new(start_world()));
        let source = {
            let world = world.clone();
            Box::new(move || world.lock().unwrap_or_else(|e| e.into_inner()).scene())
        };
        let overlay = start(None, source).expect("phone");
        let fonts = Fonts::load(None).expect("fonts");

        let mut win = Window::new(
            "Leonida (simulador) - Spotibuy",
            W,
            H,
            WindowOptions::default(),
        )
        .expect("window");
        win.set_target_fps(60);

        let mut frame = Pixmap::new(W as u32, H as u32).unwrap();
        let mut buffer = vec![0u32; W * H];
        let mut phone: Option<(u64, Pixmap)> = None;
        let mut last = Instant::now();
        while win.is_open() && !win.is_key_down(Key::Escape) {
            let now = Instant::now();
            let dt = (now - last).as_secs_f32().min(0.1);
            last = now;
            let scene = {
                let mut w = world.lock().unwrap_or_else(|e| e.into_inner());
                w.step(&win, dt);
                paint_world(&mut frame, &w, &fonts);
                w.scene()
            };
            hud(&mut frame, &fonts, &scene);

            // The phone, as the title's overlay would lay it.
            let known = phone.as_ref().map(|(s, _)| *s);
            match overlay.picture(W as u32, H as u32, known) {
                Drawn::Hidden => phone = None,
                Drawn::Same => {}
                Drawn::New(f) => {
                    phone = IntSize::from_wh(W as u32, H as u32)
                        .and_then(|size| Pixmap::from_vec(f.pixels, size))
                        .map(|p| (f.serial, p));
                }
            }
            if let Some((_, p)) = &phone {
                frame.draw_pixmap(
                    0,
                    0,
                    p.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }

            for (dst, px) in buffer.iter_mut().zip(frame.pixels()) {
                *dst =
                    u32::from(px.red()) << 16 | u32::from(px.green()) << 8 | u32::from(px.blue());
            }
            win.update_with_buffer(&buffer, W, H).expect("present");
        }
    }

    /// Screen position of world point `p`, the player at the centre.
    fn to_screen(w: &World, p: (f32, f32)) -> (f32, f32) {
        (
            W as f32 / 2.0 + (p.0 - w.player.0) * ZOOM,
            H as f32 / 2.0 + (p.1 - w.player.1) * ZOOM,
        )
    }

    fn paint_world(pix: &mut Pixmap, w: &World, fonts: &Fonts) {
        pix.fill(draw::rgb(0x15101F, 1.0));
        // Blocks around the player.
        let span = (W as f32 / ZOOM / BLOCK).ceil() as i32 + 2;
        let (bx, by) = (
            (w.player.0 / BLOCK).floor() as i32,
            (w.player.1 / BLOCK).floor() as i32,
        );
        let building = draw::solid(draw::rgb(0x2A1745, 1.0));
        let roof = draw::solid(draw::rgb(0x34205A, 1.0));
        for gx in bx - span..=bx + span {
            for gy in by - span..=by + span {
                let x0 = gx as f32 * BLOCK + ROAD / 2.0;
                let y0 = gy as f32 * BLOCK + ROAD / 2.0;
                let (sx, sy) = to_screen(w, (x0, y0));
                let size = (BLOCK - ROAD) * ZOOM;
                draw::fill(pix, draw::rounded(sx, sy, size, size, 6.0), &building);
                draw::fill(
                    pix,
                    draw::rounded(sx + 8.0, sy + 8.0, size - 16.0, size - 16.0, 4.0),
                    &roof,
                );
            }
        }
        // Road markings.
        let dash = draw::solid(draw::rgb(0x5A4A70, 1.0));
        for gx in bx - span..=bx + span {
            for k in -40..40 {
                let x = gx as f32 * BLOCK;
                let y = (by as f32 + k as f32 * 0.25) * BLOCK;
                let (sx, sy) = to_screen(w, (x, y));
                draw::fill(pix, draw::rect(sx - 1.5, sy, 3.0, 2.5 * ZOOM), &dash);
                let (sx, sy) = to_screen(w, (y, gx as f32 * BLOCK));
                draw::fill(pix, draw::rect(sx, sy - 1.5, 2.5 * ZOOM, 3.0), &dash);
            }
        }
        // How far the radio carries.
        let (cx, cy) = to_screen(w, w.car);
        draw::fill(
            pix,
            draw::circle(cx, cy, HEARING * ZOOM),
            &draw::solid(draw::rgb(0xFF2E88, 0.07)),
        );
        // The car.
        let car = Rect::from_xywh(cx - 1.0 * ZOOM, cy - 2.3 * ZOOM, 2.0 * ZOOM, 4.6 * ZOOM)
            .map(PathBuilder::from_rect);
        let t = Transform::from_rotate_at(w.car_heading.to_degrees(), cx, cy);
        if let Some(path) = &car {
            let mut paint: Paint = draw::linear(
                cx,
                cy - 2.3 * ZOOM,
                cx,
                cy + 2.3 * ZOOM,
                &[
                    (0.0, draw::rgb(0xFF2E88, 1.0)),
                    (1.0, draw::rgb(0x8B3DFF, 1.0)),
                ],
            );
            paint.shader.transform(t);
            pix.fill_path(path, &paint, FillRule::Winding, t, None);
        }
        let lights = draw::solid(draw::rgb(0xFFF2B0, 1.0));
        for side in [-0.6f32, 0.6] {
            if let Some(p) = draw::circle(cx + side * ZOOM, cy - 2.1 * ZOOM, 0.35 * ZOOM) {
                pix.fill_path(&p, &lights, FillRule::Winding, t, None);
            }
        }
        // The player, on foot.
        if !w.in_car {
            let (px, py) = to_screen(w, w.player);
            let (s, c) = w.heading.sin_cos();
            let tip = (px + s * 1.6 * ZOOM, py - c * 1.6 * ZOOM);
            let l = (px - c * 0.6 * ZOOM, py - s * 0.6 * ZOOM);
            let r = (px + c * 0.6 * ZOOM, py + s * 0.6 * ZOOM);
            draw::fill(
                pix,
                draw::polygon(&[tip, l, r]),
                &draw::solid(draw::rgb(0x5BE0FF, 0.9)),
            );
            draw::fill(
                pix,
                draw::circle(px, py, 0.55 * ZOOM),
                &draw::solid(draw::rgb(0xF7F3FF, 1.0)),
            );
        } else {
            let _ = fonts;
        }
        if w.paused {
            draw::fill(
                pix,
                draw::rect(0.0, 0.0, W as f32, H as f32),
                &draw::solid(draw::rgb(0x000000, 0.55)),
            );
            let ink = draw::ink(draw::rgb(0xF7F3FF, 1.0));
            let size = 46.0;
            let width = draw::measure(&fonts.display, size, "PAUSADO");
            draw::text(
                pix,
                &fonts.display,
                size,
                (W as f32 - width) / 2.0,
                H as f32 / 2.0,
                "PAUSADO",
                &ink,
            );
        }
    }

    fn hud(pix: &mut Pixmap, fonts: &Fonts, scene: &Scene) {
        draw::fill(
            pix,
            draw::rounded(16.0, 16.0, 470.0, 150.0, 14.0),
            &draw::solid(draw::rgb(0x1A0B2E, 0.82)),
        );
        let pearl = draw::ink(draw::rgb(0xF7F3FF, 1.0));
        let muted = draw::ink(draw::rgb(0xB9A9D6, 1.0));
        let pool = draw::ink(draw::rgb(0x5BE0FF, 1.0));
        draw::text(
            pix,
            &fonts.display,
            20.0,
            32.0,
            46.0,
            "Leonida (simulador)",
            &pearl,
        );
        draw::text(pix, &fonts.semi, 15.0, 32.0, 74.0, &label(scene), &pool);
        let (pan, back) = place(scene);
        let (l, r) = gains(pan, back);
        let side = if scene.in_vehicle {
            "em volta (dentro do carro)".to_string()
        } else {
            let deg = scene.bearing.to_degrees();
            let where_ = match deg {
                d if d.abs() <= 45.0 => "na frente",
                d if d.abs() >= 135.0 => "atrás",
                d if d > 0.0 => "à direita",
                _ => "à esquerda",
            };
            format!("o carro está {where_}")
        };
        draw::text(pix, &fonts.text, 14.0, 32.0, 98.0, &side, &muted);
        draw::text(
            pix,
            &fonts.text,
            14.0,
            32.0,
            120.0,
            &format!("balanço: esquerda {l:.2} · direita {r:.2}"),
            &muted,
        );
        let help =
            "W A S D andar/dirigir · E entrar/sair · T ligação · P pausa · F7 celular · Esc sair";
        draw::text(pix, &fonts.text, 12.0, 32.0, 148.0, help, &muted);
        let _ = PI;
    }
}
