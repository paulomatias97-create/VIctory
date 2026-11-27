//! The Spotibuy phone: what Spotify plays, drawn as a phone that slides in
//! over the title's picture. The picture is transparent around the phone
//! and premultiplied, ready to be laid over a frame.
//!
//! Sizes are in phone units (the phone is 360 by 740) scaled to the
//! output: at 1080 lines the phone is 740 pixels tall.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use tiny_skia::{Color, FilterQuality, Paint, Pattern, Pixmap, SpreadMode, Transform};

use super::draw::{self, rgb};
use super::fonts::Fonts;

const NIGHT: u32 = 0x1A0B2E;
const CARD: u32 = 0x241140;
const PINK: u32 = 0xFF2E88;
const PURPLE: u32 = 0x8B3DFF;
const VIOLET: u32 = 0xB04CFF;
const POOL: u32 = 0x5BE0FF;
const PEARL: u32 = 0xF7F3FF;
const MUTED: u32 = 0xB9A9D6;

/// What the phone shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// The cover as Spotify hands it (JPEG or PNG).
    pub cover: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct PhoneState {
    /// How far the phone has slid in: 0 hidden, 1 shown.
    pub open: f32,
    /// The time on the status bar ("21:47").
    pub clock: String,
    /// What Spotify plays, `None` when it is not open.
    pub track: Option<Track>,
    /// The music's level as the scene sets it (0 to 1).
    pub level: f32,
    /// The scene in a word or two ("No carro", "Diálogo").
    pub scene: String,
    /// Seconds since the phone opened, for the equalizer and the jokes.
    pub time: f32,
}

/// Lines about the platform (never about the artist playing).
const JOKES: [&str; 6] = [
    "Seu algoritmo já foi pago.",
    "Refrão em 7 segundos. Ninguém aguenta introdução.",
    "Pular anúncio? Plano Premium Ouro Diamante.",
    "Ouvintes mensais: muitos. Fãs reais: calculando...",
    "Modo Descoberta: quer ser ouvido? Receba menos.",
    "Wrapped do ano: você ouviu o que mandaram.",
];

fn joke_index(time: f32) -> usize {
    (time.max(0.0) / 8.0) as usize % JOKES.len()
}

pub struct Phone {
    fonts: Fonts,
    cover: Option<(u64, Pixmap)>,
    /// What does not move between frames (all but the equalizer and the
    /// level), drawn with the phone fully in, by what it shows.
    base: Option<(u64, Pixmap)>,
}

/// The phone's size in its own units.
pub const W: f32 = 360.0;
pub const H: f32 = 740.0;

impl Phone {
    pub fn new(fonts: Fonts) -> Self {
        Self {
            fonts,
            cover: None,
            base: None,
        }
    }

    /// The overlay for a `width` by `height` output: transparent, with the
    /// phone on the right. `None` while the phone is fully hidden.
    pub fn render(&mut self, state: &PhoneState, width: u32, height: u32) -> Option<Pixmap> {
        if state.open <= 0.0 {
            return None;
        }
        let s = height as f32 / 1080.0;
        let margin = 48.0 * s;
        let ox = width as f32 - margin - W * s;
        let oy = (height as f32 - H * s) / 2.0;
        // Slide in from the right, easing out.
        let t = 1.0 - (1.0 - state.open.clamp(0.0, 1.0)).powi(3);
        let dx = ((1.0 - t) * (W * s + margin)).round();

        let key = {
            let mut h = DefaultHasher::new();
            (width, height, &state.clock, joke_index(state.time)).hash(&mut h);
            if let Some(tr) = &state.track {
                (&tr.title, &tr.artist, tr.playing, &tr.cover).hash(&mut h);
            }
            h.finish()
        };
        if self.base.as_ref().is_none_or(|(k, _)| *k != key) {
            let cover = state
                .track
                .as_ref()
                .and_then(|tr| tr.cover.as_deref())
                .and_then(|bytes| self.cover_pixmap(bytes));
            let mut base = Pixmap::new(width, height)?;
            Painter {
                pix: &mut base,
                fonts: &self.fonts,
                s,
                ox,
                oy,
            }
            .still(state, cover.as_ref());
            self.base = Some((key, base));
        }
        let base = &self.base.as_ref()?.1;
        let mut pix = if dx == 0.0 {
            base.clone()
        } else {
            let mut p = Pixmap::new(width, height)?;
            p.draw_pixmap(
                dx as i32,
                0,
                base.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            p
        };
        Painter {
            pix: &mut pix,
            fonts: &self.fonts,
            s,
            ox: ox + dx,
            oy,
        }
        .moving(state);
        Some(pix)
    }

    /// The decoded cover, kept while the same bytes come back.
    fn cover_pixmap(&mut self, bytes: &[u8]) -> Option<Pixmap> {
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        let key = h.finish();
        if let Some((k, p)) = &self.cover
            && *k == key
        {
            return Some(p.clone());
        }
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let mut p = Pixmap::new(w, h)?;
        for (dst, src) in p
            .data_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(img.pixels())
        {
            let a = u16::from(src[3]);
            for c in 0..3 {
                dst[c] = ((u16::from(src[c]) * a + 127) / 255) as u8;
            }
            dst[3] = src[3];
        }
        self.cover = Some((key, p.clone()));
        Some(p)
    }
}

struct Painter<'a> {
    pix: &'a mut Pixmap,
    fonts: &'a Fonts,
    s: f32,
    ox: f32,
    oy: f32,
}

impl<'a> Painter<'a> {
    fn x(&self, x: f32) -> f32 {
        self.ox + x * self.s
    }
    fn y(&self, y: f32) -> f32 {
        self.oy + y * self.s
    }
    fn d(&self, v: f32) -> f32 {
        v * self.s
    }

    fn rounded(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, paint: &Paint) {
        let path = draw::rounded(self.x(x), self.y(y), self.d(w), self.d(h), self.d(r));
        draw::fill(self.pix, path, paint);
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, paint: &Paint) {
        let path = draw::circle(self.x(cx), self.y(cy), self.d(r));
        draw::fill(self.pix, path, paint);
    }

    /// A horizontal gradient paint across phone units `x0..x1`.
    fn across(&self, x0: f32, x1: f32, stops: &[(f32, Color)]) -> Paint<'static> {
        draw::linear(self.x(x0), 0.0, self.x(x1), 0.0, stops)
    }

    fn down(&self, y0: f32, y1: f32, stops: &[(f32, Color)]) -> Paint<'static> {
        draw::linear(0.0, self.y(y0), 0.0, self.y(y1), stops)
    }

    /// Text in phone units; `align` 0 left, 0.5 centre, 1 right of `x`.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        font: Face,
        px: f32,
        x: f32,
        y: f32,
        s: &str,
        max: f32,
        align: f32,
        ink: &dyn Fn(f32, f32) -> [f32; 4],
    ) {
        let f = self.font(font);
        let px = self.d(px);
        let s = draw::fit(f, px, s, self.d(max));
        let w = draw::measure(f, px, &s);
        let (tx, ty) = (self.x(x) - w * align, self.y(y));
        draw::text(self.pix, f, px, tx, ty, &s, ink);
    }

    fn font(&self, face: Face) -> &'a fontdue::Font {
        let fonts: &'a Fonts = self.fonts;
        match face {
            Face::Display => &fonts.display,
            Face::Bold => &fonts.bold,
            Face::Text => &fonts.text,
            Face::Semi => &fonts.semi,
        }
    }

    /// Everything that stays still while the same song plays.
    fn still(&mut self, st: &PhoneState, cover: Option<&Pixmap>) {
        // Body and screen.
        let shadow = draw::solid(rgb(0x000000, 0.45));
        let (sx, sy, sw, sh) = (self.x(-6.0), self.y(10.0), self.d(W + 12.0), self.d(H));
        draw::glow(
            self.pix,
            (sx, sy, sw, sh),
            self.d(28.0) as u32,
            1,
            |l, ox, oy| {
                draw::fill(l, draw::rounded(sx - ox, sy - oy, sw, sh, 60.0), &shadow);
            },
        );
        let body = self.down(
            0.0,
            H,
            &[(0.0, rgb(0x3A2A52, 1.0)), (1.0, rgb(0x120720, 1.0))],
        );
        self.rounded(0.0, 0.0, W, H, 50.0, &body);
        self.rounded(
            10.0,
            10.0,
            W - 20.0,
            H - 20.0,
            41.0,
            &draw::solid(rgb(NIGHT, 1.0)),
        );
        // A pink haze at the top of the screen.
        let haze = self.down(
            10.0,
            300.0,
            &[(0.0, rgb(PINK, 0.22)), (1.0, rgb(PINK, 0.0))],
        );
        self.rounded(10.0, 10.0, W - 20.0, 290.0, 41.0, &haze);
        self.rounded(
            130.0,
            20.0,
            100.0,
            26.0,
            13.0,
            &draw::solid(rgb(0x000000, 1.0)),
        );

        let pearl = draw::ink(rgb(PEARL, 1.0));
        let muted = draw::ink(rgb(MUTED, 1.0));
        self.text(Face::Semi, 14.0, 38.0, 40.0, &st.clock, 80.0, 0.0, &pearl);
        self.text(Face::Semi, 13.0, 322.0, 40.0, "5G", 60.0, 1.0, &pearl);

        // Header: the logo and the name.
        self.logo(44.0, 82.0, 15.0);
        let name = draw::ink_across(
            self.x(66.0),
            self.x(170.0),
            [rgb(0xFF7AC0, 1.0), rgb(VIOLET, 1.0), rgb(POOL, 1.0)],
        );
        self.text(
            Face::Display,
            21.0,
            66.0,
            90.0,
            "Spotibuy",
            140.0,
            0.0,
            &name,
        );
        self.text(
            Face::Semi,
            10.0,
            320.0,
            88.0,
            "TOCANDO AGORA",
            120.0,
            1.0,
            &muted,
        );

        let Some(track) = &st.track else {
            self.cover_box(None);
            self.text(
                Face::Bold,
                21.0,
                180.0,
                448.0,
                "Abra o Spotify",
                280.0,
                0.5,
                &pearl,
            );
            self.text(
                Face::Text,
                14.0,
                180.0,
                474.0,
                "e dê play: a música aparece aqui.",
                290.0,
                0.5,
                &muted,
            );
            self.footer();
            return;
        };
        self.cover_box(cover);
        self.text(
            Face::Bold,
            22.0,
            40.0,
            440.0,
            &track.title,
            280.0,
            0.0,
            &pearl,
        );
        self.text(
            Face::Text,
            15.0,
            40.0,
            465.0,
            &track.artist,
            280.0,
            0.0,
            &muted,
        );

        self.rounded(
            40.0,
            512.0,
            280.0,
            5.0,
            2.5,
            &draw::solid(rgb(0x2F1852, 1.0)),
        );
        self.controls(track.playing);

        // A line about the platform, changing every eight seconds.
        let joke = JOKES[joke_index(st.time)];
        self.rounded(40.0, 628.0, 280.0, 44.0, 14.0, &draw::solid(rgb(CARD, 1.0)));
        self.text(Face::Text, 12.0, 180.0, 655.0, joke, 260.0, 0.5, &muted);
        self.footer();
    }

    /// What moves every frame: the equalizer, the scene and its level.
    fn moving(&mut self, st: &PhoneState) {
        let Some(track) = &st.track else {
            return;
        };
        self.equalizer(40.0, 482.0, track.playing, st.time);
        let line = format!(
            "{} · {} · {}%",
            if track.playing { "Tocando" } else { "Pausado" },
            st.scene,
            (st.level * 100.0).round() as u32
        );
        self.text(
            Face::Semi,
            12.0,
            92.0,
            496.0,
            &line,
            230.0,
            0.0,
            &draw::ink(rgb(POOL, 1.0)),
        );
        if st.level > 0.01 {
            let fill = self.across(40.0, 320.0, &[(0.0, rgb(POOL, 1.0)), (1.0, rgb(PINK, 1.0))]);
            self.rounded(
                40.0,
                512.0,
                280.0 * st.level.clamp(0.0, 1.0),
                5.0,
                2.5,
                &fill,
            );
        }
    }

    /// The neon S badge, centred at `(cx, cy)`, of radius `r`.
    fn logo(&mut self, cx: f32, cy: f32, r: f32) {
        let ring = draw::linear(
            self.x(cx - r),
            self.y(cy - r),
            self.x(cx + r),
            self.y(cy + r),
            &[(0.0, rgb(PINK, 1.0)), (1.0, rgb(PURPLE, 1.0))],
        );
        self.circle(cx, cy, r, &ring);
        self.circle(cx, cy, r * 0.88, &draw::solid(rgb(NIGHT, 1.0)));
        let (top, bottom) = (self.y(cy - r * 0.7), self.y(cy + r * 0.7));
        let tube = move |_: f32, y: f32| {
            let t = ((y - top) / (bottom - top)).clamp(0.0, 1.0);
            let (a, b) = (rgb(0xFF7AC0, 1.0), rgb(POOL, 1.0));
            [
                a.red() + (b.red() - a.red()) * t,
                a.green() + (b.green() - a.green()) * t,
                a.blue() + (b.blue() - a.blue()) * t,
                1.0,
            ]
        };
        let px = r * 1.35;
        let font: &'a fontdue::Font = &self.fonts.display;
        let size = self.d(px);
        let w = draw::measure(font, size, "S");
        let (tx, ty) = (self.x(cx) - w / 2.0, self.y(cy + px * 0.36));
        let violet = draw::ink(rgb(VIOLET, 1.0));
        let radius = self.d(r * 0.5) as u32;
        let bounds = (tx, ty - size, w, size * 1.2);
        draw::glow(self.pix, bounds, radius, 2, |l, ox, oy| {
            draw::text(l, font, size, tx - ox, ty - oy, "S", &violet);
        });
        draw::text(self.pix, font, size, tx, ty, "S", &tube);
    }

    /// The cover (or a stand-in) at the top of the screen.
    fn cover_box(&mut self, cover: Option<&Pixmap>) {
        let (x, y, size, r) = (40.0, 110.0, 280.0, 20.0);
        let (px, py, ps) = (self.x(x), self.y(y), self.d(size));
        let rr = self.d(r);
        draw::glow(
            self.pix,
            (px, py, ps, ps + rr),
            self.d(22.0) as u32,
            1,
            |l, ox, oy| {
                let halo = draw::linear(
                    px - ox,
                    py - oy,
                    px - ox + ps,
                    py - oy + ps,
                    &[(0.0, rgb(PINK, 0.35)), (1.0, rgb(PURPLE, 0.35))],
                );
                draw::fill(
                    l,
                    draw::rounded(px - ox, py - oy, ps, ps, rr),
                    &halo,
                );
            },
        );
        match cover {
            Some(c) => {
                // Fill the square, cropping the longer side.
                let scale = ps / c.width().min(c.height()) as f32;
                let tx = px - (c.width() as f32 * scale - ps) / 2.0;
                let ty = py - (c.height() as f32 * scale - ps) / 2.0;
                let paint = Paint {
                    shader: Pattern::new(
                        c.as_ref(),
                        SpreadMode::Pad,
                        FilterQuality::Bicubic,
                        1.0,
                        Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
                    ),
                    anti_alias: true,
                    ..Paint::default()
                };
                self.rounded(x, y, size, size, r, &paint);
            }
            None => {
                let bg = draw::linear(
                    px,
                    py,
                    px + ps,
                    py + ps,
                    &[(0.0, rgb(PINK, 1.0)), (1.0, rgb(PURPLE, 1.0))],
                );
                self.rounded(x, y, size, size, r, &bg);
                self.logo(x + size / 2.0, y + size / 2.0, 70.0);
            }
        }
    }

    /// Seven bars, moving while the music plays.
    fn equalizer(&mut self, x: f32, y: f32, playing: bool, time: f32) {
        for i in 0..7 {
            let phase = time * (3.0 + i as f32 * 0.7) + i as f32 * 1.3;
            let h = if playing {
                5.0 + 11.0 * (0.5 + 0.5 * phase.sin())
            } else {
                4.0
            };
            let c = if i < 3 {
                PINK
            } else if i < 5 {
                VIOLET
            } else {
                POOL
            };
            self.rounded(
                x + i as f32 * 6.5,
                y + 16.0 - h,
                4.0,
                h,
                2.0,
                &draw::solid(rgb(c, 1.0)),
            );
        }
    }

    fn controls(&mut self, playing: bool) {
        let (cy, pearl) = (570.0, draw::solid(rgb(PEARL, 1.0)));
        // Previous: a bar and a triangle pointing left.
        self.rounded(86.0, cy - 11.0, 4.0, 22.0, 2.0, &pearl);
        self.poly(
            &[(112.0, cy - 12.0), (112.0, cy + 12.0), (92.0, cy)],
            &pearl,
        );
        // Next.
        self.rounded(270.0, cy - 11.0, 4.0, 22.0, 2.0, &pearl);
        self.poly(
            &[(248.0, cy - 12.0), (248.0, cy + 12.0), (268.0, cy)],
            &pearl,
        );
        // Play or pause: a neon disc.
        let (cx, r) = (180.0, 36.0);
        let (x0, y0, x1, y1) = (
            self.x(cx - r),
            self.y(cy - r),
            self.x(cx + r),
            self.y(cy + r),
        );
        let (gx, gy, gr) = (self.x(cx), self.y(cy), self.d(r));
        draw::glow(
            self.pix,
            (x0, y0, x1 - x0, y1 - y0),
            self.d(18.0) as u32,
            1,
            |l, ox, oy| {
                let disc = draw::linear(
                    x0 - ox,
                    y0 - oy,
                    x1 - ox,
                    y1 - oy,
                    &[
                        (0.0, rgb(PINK, 1.0)),
                        (0.6, rgb(PURPLE, 1.0)),
                        (1.0, rgb(POOL, 1.0)),
                    ],
                );
                draw::fill(l, draw::circle(gx - ox, gy - oy, gr), &disc);
            },
        );
        let white = draw::solid(rgb(0xFFFFFF, 1.0));
        if playing {
            self.rounded(cx - 11.0, cy - 13.0, 8.0, 26.0, 3.0, &white);
            self.rounded(cx + 3.0, cy - 13.0, 8.0, 26.0, 3.0, &white);
        } else {
            self.poly(
                &[
                    (cx - 9.0, cy - 15.0),
                    (cx - 9.0, cy + 15.0),
                    (cx + 15.0, cy),
                ],
                &white,
            );
        }
    }

    fn poly(&mut self, pts: &[(f32, f32)], paint: &Paint) {
        let pts: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| (self.x(x), self.y(y))).collect();
        draw::fill(self.pix, draw::polygon(&pts), paint);
    }

    fn footer(&mut self) {
        let muted = draw::ink(rgb(MUTED, 0.75));
        self.text(
            Face::Text,
            10.5,
            180.0,
            694.0,
            "F7 fechar · F8 play/pausa · F9 próxima · F6 anterior",
            300.0,
            0.5,
            &muted,
        );
        self.text(
            Face::Text,
            9.5,
            180.0,
            712.0,
            "música e capa via Spotify",
            300.0,
            0.5,
            &draw::ink(rgb(MUTED, 0.5)),
        );
    }
}

#[derive(Clone, Copy)]
enum Face {
    Display,
    Bold,
    Text,
    Semi,
}
