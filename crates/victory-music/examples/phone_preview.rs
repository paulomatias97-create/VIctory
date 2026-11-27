//! Draws the phone as the overlay would show it, over a stand-in frame, and
//! saves a PNG (any platform):
//!
//! ```text
//! VICTORY_FONTS_DIR=<folder with Rubik and Inter> \
//!   cargo run -p victory-music --example phone_preview -- out.png [cover.jpg] [paused|empty]
//! ```
//!
//! The overlay alone (transparent around the phone) goes to `out.overlay.png`.

use tiny_skia::{Pixmap, PixmapPaint, Transform};
use victory_music::ui::{Fonts, Phone, PhoneState, Track, draw};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "phone.png".into());
    let cover_path = args.next();
    let mode = args.next().unwrap_or_default();
    let fonts = Fonts::load(None).expect("fonts");
    let mut phone = Phone::new(fonts);

    let (w, h) = (1920, 1080);
    let cover = cover_path
        .filter(|p| !p.is_empty() && p != "-")
        .map(|p| std::fs::read(p).expect("cover"))
        .or_else(|| stand_in_cover().encode_png().ok());
    let track = (mode != "empty").then(|| Track {
        title: "Noite em Leonida (Remix)".into(),
        artist: "Artista de Exemplo".into(),
        playing: mode != "paused",
        cover,
    });
    let state = PhoneState {
        open: 1.0,
        clock: "21:47".into(),
        track,
        level: 1.0,
        scene: "No carro".into(),
        time: 3.2,
    };
    let start = std::time::Instant::now();
    let overlay = phone.render(&state, w, h).expect("overlay");
    eprintln!("first drawing (decodes the cover): {:?}", start.elapsed());
    let again = std::time::Instant::now();
    for _ in 0..10 {
        let _ = phone.render(&state, w, h);
    }
    eprintln!("each later drawing: {:?}", again.elapsed() / 10);
    overlay
        .save_png(out.replace(".png", ".overlay.png"))
        .expect("save overlay");

    let mut frame = stand_in_frame(w, h);
    frame.draw_pixmap(
        0,
        0,
        overlay.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    frame.save_png(&out).expect("save");
    eprintln!("saved {out}");
}

/// A night street of our own drawing, to see the phone over something.
fn stand_in_frame(w: u32, h: u32) -> Pixmap {
    let mut p = Pixmap::new(w, h).unwrap();
    let (wf, hf) = (w as f32, h as f32);
    let sky = draw::linear(
        0.0,
        0.0,
        0.0,
        hf,
        &[
            (0.0, draw::rgb(0x0B1030, 1.0)),
            (0.55, draw::rgb(0x5A1E6E, 1.0)),
            (0.7, draw::rgb(0xE0567A, 1.0)),
            (1.0, draw::rgb(0x1A0F24, 1.0)),
        ],
    );
    draw::fill(&mut p, draw::rect(0.0, 0.0, wf, hf), &sky);
    // Buildings.
    let mut x = 0.0;
    let mut i = 0u32;
    while x < wf {
        let bw = 90.0 + ((i * 53) % 80) as f32;
        let bh = 220.0 + ((i * 97) % 300) as f32;
        draw::fill(
            &mut p,
            draw::rect(x, hf * 0.72 - bh, bw - 8.0, bh),
            &draw::solid(draw::rgb(0x120A1E, 1.0)),
        );
        for wy in (0..(bh as u32 / 28)).map(|k| hf * 0.72 - bh + 14.0 + k as f32 * 28.0) {
            for wx in (0..((bw as u32 - 20) / 18)).map(|k| x + 10.0 + k as f32 * 18.0) {
                if (wx as u32 + wy as u32 + i).is_multiple_of(3) {
                    draw::fill(
                        &mut p,
                        draw::rect(wx, wy, 8.0, 12.0),
                        &draw::solid(draw::rgb(0xFFD27A, 0.7)),
                    );
                }
            }
        }
        x += bw;
        i += 1;
    }
    draw::fill(
        &mut p,
        draw::rect(0.0, hf * 0.72, wf, hf * 0.28),
        &draw::solid(draw::rgb(0x0E0816, 1.0)),
    );
    p
}

/// A cover of our own drawing: a sun over the sea.
fn stand_in_cover() -> Pixmap {
    let mut p = Pixmap::new(600, 600).unwrap();
    let sky = draw::linear(
        0.0,
        0.0,
        0.0,
        600.0,
        &[
            (0.0, draw::rgb(0xFF7A45, 1.0)),
            (0.5, draw::rgb(0xFF2E88, 1.0)),
            (0.5, draw::rgb(0x5A1E8C, 1.0)),
            (1.0, draw::rgb(0x2A0F4A, 1.0)),
        ],
    );
    draw::fill(&mut p, draw::rect(0.0, 0.0, 600.0, 600.0), &sky);
    draw::fill(
        &mut p,
        draw::circle(400.0, 200.0, 95.0),
        &draw::solid(draw::rgb(0xFFB347, 1.0)),
    );
    for k in 0..6 {
        let y = 330.0 + k as f32 * 38.0;
        draw::fill(
            &mut p,
            draw::rect(320.0 - k as f32 * 8.0, y, 160.0 + k as f32 * 16.0, 10.0),
            &draw::solid(draw::rgb(0xFFB347, 0.5 - k as f32 * 0.07)),
        );
    }
    p
}
