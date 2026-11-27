//! Drawing on a premultiplied RGBA pixmap (what the presentation
//! compositor blends with `ONE, INV_SRC_ALPHA`): shapes through tiny-skia,
//! text through fontdue, and a blur for neon glows.

use fontdue::Font;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, Point, Rect,
    Shader, SpreadMode, Transform,
};

/// An sRGB colour from `0xRRGGBB` and an opacity.
pub fn rgb(hex: u32, alpha: f32) -> Color {
    Color::from_rgba8(
        (hex >> 16) as u8,
        (hex >> 8) as u8,
        hex as u8,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

pub fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

/// A linear gradient from `(x0, y0)` to `(x1, y1)` through `stops`
/// (position, colour).
pub fn linear(x0: f32, y0: f32, x1: f32, y1: f32, stops: &[(f32, Color)]) -> Paint<'static> {
    let stops = stops
        .iter()
        .map(|&(p, c)| GradientStop::new(p, c))
        .collect();
    let shader = LinearGradient::new(
        Point::from_xy(x0, y0),
        Point::from_xy(x1, y1),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or(Shader::SolidColor(Color::TRANSPARENT));
    Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    }
}

/// A rectangle with corners of radius `r`.
pub fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    // Cubic approximation of a quarter circle.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

pub fn circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    PathBuilder::from_circle(cx, cy, r)
}

pub fn polygon(points: &[(f32, f32)]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (&(x, y), rest) = points.split_first()?;
    pb.move_to(x, y);
    for &(x, y) in rest {
        pb.line_to(x, y);
    }
    pb.close();
    pb.finish()
}

pub fn fill(pix: &mut Pixmap, path: Option<Path>, paint: &Paint) {
    if let Some(path) = path {
        pix.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    Rect::from_xywh(x, y, w, h).map(PathBuilder::from_rect)
}

/// The width of `s` at `px`.
pub fn measure(font: &Font, px: f32, s: &str) -> f32 {
    let mut w = 0.0;
    let mut prev = None;
    for c in s.chars() {
        if let Some(p) = prev {
            w += font.horizontal_kern(p, c, px).unwrap_or(0.0);
        }
        w += font.metrics(c, px).advance_width;
        prev = Some(c);
    }
    w
}

/// `s` cut with an ellipsis to fit `max` (unchanged when it fits).
pub fn fit(font: &Font, px: f32, s: &str, max: f32) -> String {
    if measure(font, px, s) <= max {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() {
        out.pop();
        let t = format!("{}…", out.trim_end());
        if measure(font, px, &t) <= max {
            return t;
        }
    }
    "…".into()
}

/// Draws `s` with its baseline at `y`, starting at `x`, each pixel's
/// colour given by `ink(x, y)` (straight RGBA, 0 to 1). Returns the width.
pub fn text(
    pix: &mut Pixmap,
    font: &Font,
    px: f32,
    x: f32,
    y: f32,
    s: &str,
    ink: &dyn Fn(f32, f32) -> [f32; 4],
) -> f32 {
    let (w, h) = (pix.width() as i32, pix.height() as i32);
    let data = pix.data_mut();
    let mut pen = x;
    let mut prev = None;
    for c in s.chars() {
        if let Some(p) = prev {
            pen += font.horizontal_kern(p, c, px).unwrap_or(0.0);
        }
        let (m, cov) = font.rasterize(c, px);
        let gx = (pen + m.xmin as f32).round() as i32;
        let gy = (y - (m.height as i32 + m.ymin) as f32).round() as i32;
        for row in 0..m.height as i32 {
            let py = gy + row;
            if py < 0 || py >= h {
                continue;
            }
            for col in 0..m.width as i32 {
                let px_ = gx + col;
                if px_ < 0 || px_ >= w {
                    continue;
                }
                let a = f32::from(cov[(row * m.width as i32 + col) as usize]) / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let [r, g, b, alpha] = ink(px_ as f32, py as f32);
                blend(data, ((py * w + px_) * 4) as usize, [r, g, b, alpha * a]);
            }
        }
        pen += m.advance_width;
        prev = Some(c);
    }
    pen - x
}

/// Source-over of a straight colour onto a premultiplied pixel.
fn blend(data: &mut [u8], i: usize, [r, g, b, a]: [f32; 4]) {
    let inv = 1.0 - a;
    let mix = |src: f32, dst: u8| {
        (src * a * 255.0 + f32::from(dst) * inv)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    data[i] = mix(r, data[i]);
    data[i + 1] = mix(g, data[i + 1]);
    data[i + 2] = mix(b, data[i + 2]);
    data[i + 3] = (a * 255.0 + f32::from(data[i + 3]) * inv)
        .round()
        .clamp(0.0, 255.0) as u8;
}

/// A straight RGBA ink of one colour.
pub fn ink(c: Color) -> impl Fn(f32, f32) -> [f32; 4] {
    move |_, _| [c.red(), c.green(), c.blue(), c.alpha()]
}

/// A horizontal gradient ink from `a` at `x0` to `b` at `x1`, through `mid`.
pub fn ink_across(x0: f32, x1: f32, stops: [Color; 3]) -> impl Fn(f32, f32) -> [f32; 4] {
    move |x, _| {
        let t = ((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
        let (a, b, t) = if t < 0.5 {
            (stops[0], stops[1], t * 2.0)
        } else {
            (stops[1], stops[2], t * 2.0 - 1.0)
        };
        let l = |p: f32, q: f32| p + (q - p) * t;
        [
            l(a.red(), b.red()),
            l(a.green(), b.green()),
            l(a.blue(), b.blue()),
            l(a.alpha(), b.alpha()),
        ]
    }
}

/// Blurs `pix` in place: three box passes each way (close to a Gaussian of
/// about `radius`).
pub fn blur(pix: &mut Pixmap, radius: u32) {
    if radius == 0 {
        return;
    }
    let (w, h) = (pix.width() as usize, pix.height() as usize);
    let data = pix.data_mut();
    let mut tmp = vec![0u8; data.len()];
    let r = (radius / 2).max(1) as usize;
    for _ in 0..3 {
        box_pass(data, &mut tmp, w, h, r, true);
        box_pass(&tmp, data, w, h, r, false);
    }
}

fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| {
        if horizontal {
            (line * w + i) * 4
        } else {
            (i * w + line) * 4
        }
    };
    let span = (2 * r + 1) as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        for i in 0..=r.min(len - 1) {
            for (c, s) in sum.iter_mut().enumerate() {
                *s += u32::from(src[at(line, i) + c]);
            }
        }
        for i in 0..len {
            let o = at(line, i);
            for (c, s) in sum.iter().enumerate() {
                dst[o + c] = (s / span) as u8;
            }
            if i + r + 1 < len {
                let n = at(line, i + r + 1);
                for (c, s) in sum.iter_mut().enumerate() {
                    *s += u32::from(src[n + c]);
                }
            }
            if i >= r {
                let old = at(line, i - r);
                for (c, s) in sum.iter_mut().enumerate() {
                    *s -= u32::from(src[old + c]);
                }
            }
        }
    }
}

/// Draws a glow under what `paint_layer` draws inside `bounds` (x, y,
/// width, height on `pix`): the layer blurred by `radius` and laid down
/// `strength` times, then the layer itself. Only `bounds` plus the blur's
/// reach is worked on; `paint_layer` gets a layer whose origin is at
/// `(ox, oy)` on `pix` and draws at its own coordinates minus those.
pub fn glow(
    pix: &mut Pixmap,
    bounds: (f32, f32, f32, f32),
    radius: u32,
    strength: u32,
    paint_layer: impl Fn(&mut Pixmap, f32, f32),
) {
    let reach = (radius * 2) as f32;
    let ox = (bounds.0 - reach).floor().max(0.0);
    let oy = (bounds.1 - reach).floor().max(0.0);
    let x1 = (bounds.0 + bounds.2 + reach).ceil().min(pix.width() as f32);
    let y1 = (bounds.1 + bounds.3 + reach)
        .ceil()
        .min(pix.height() as f32);
    if x1 <= ox || y1 <= oy {
        return;
    }
    let Some(mut layer) = Pixmap::new((x1 - ox) as u32, (y1 - oy) as u32) else {
        return;
    };
    paint_layer(&mut layer, ox, oy);
    let mut halo = layer.clone();
    blur(&mut halo, radius);
    let p = tiny_skia::PixmapPaint::default();
    let (x, y) = (ox as i32, oy as i32);
    for _ in 0..strength {
        pix.draw_pixmap(x, y, halo.as_ref(), &p, Transform::identity(), None);
    }
    pix.draw_pixmap(x, y, layer.as_ref(), &p, Transform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_spreads_and_keeps_coverage() {
        let mut p = Pixmap::new(41, 41).unwrap();
        fill(&mut p, rect(18.0, 18.0, 5.0, 5.0), &solid(Color::WHITE));
        let before: u32 = p.data().chunks(4).map(|c| u32::from(c[3])).sum();
        blur(&mut p, 6);
        let after: u32 = p.data().chunks(4).map(|c| u32::from(c[3])).sum();
        // Spread out (a pixel off the square is lit) ...
        assert!(p.pixel(14, 20).unwrap().alpha() > 0);
        // ... with about the same total (box sums lose a little to rounding).
        assert!(
            after <= before && after * 10 >= before * 8,
            "{before} {after}"
        );
    }

    #[test]
    fn rounded_rects_fill_inside_not_corners() {
        let mut p = Pixmap::new(40, 40).unwrap();
        fill(
            &mut p,
            rounded(0.0, 0.0, 40.0, 40.0, 12.0),
            &solid(Color::WHITE),
        );
        assert_eq!(p.pixel(20, 20).unwrap().alpha(), 255);
        assert_eq!(p.pixel(0, 0).unwrap().alpha(), 0);
    }
}
