//! Quote image renderer, port of `src/quotes/quote.nim` (pixie) to
//! tiny-skia + ab_glyph. Images are judged by eye, not pixel equality:
//! the composition is the deployed one — jpg background, diagonal random
//! gradient tinted over it (fully opaque in color mode, 50% in black&white),
//! black body text with a white outline, and a black `@MarkinimBot`
//! watermark. PNG bytes are returned in memory (the Nim build wrote a temp
//! file and sent it via `file://`).

use ab_glyph::{Font, FontRef, Glyph, PxScale, PxScaleFont, ScaleFont};
use rand::Rng;
use tiny_skia::{ColorU8, Pixmap};

const QUOTE_WIDTH: f32 = 1000.0;
const QUOTE_HEIGHT: f32 = 1000.0;

const LORA: &[u8] = include_bytes!("quotes/Lora-BoldItalic.ttf");
const OSWALD: &[u8] = include_bytes!("quotes/Oswald-SemiBold.ttf");
const MONTSERRAT: &[u8] = include_bytes!("quotes/Montserrat-ExtraBold.ttf");
const BACKGROUND: &[u8] = include_bytes!("quotes/markinim.jpg");

pub struct QuoteAssets {
    background: Pixmap,
    body_fonts: [FontRef<'static>; 2],
    watermark_font: FontRef<'static>,
}

struct Mask {
    coverage: Vec<f32>,
    width: i32,
    height: i32,
}

impl Mask {
    fn blank() -> Self {
        Self {
            coverage: vec![0.0; (QUOTE_WIDTH * QUOTE_HEIGHT) as usize],
            width: QUOTE_WIDTH as i32,
            height: QUOTE_HEIGHT as i32,
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Background(String),
    Font(&'static str),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Background(e) => write!(f, "cannot load markinim.jpg: {e}"),
            LoadError::Font(name) => write!(f, "cannot load font {name}"),
        }
    }
}

fn load_font(bytes: &'static [u8], name: &'static str) -> Result<FontRef<'static>, LoadError> {
    FontRef::try_from_slice(bytes).map_err(|_| LoadError::Font(name))
}

impl QuoteAssets {
    pub fn load() -> Result<Self, LoadError> {
        let img = image::load_from_memory(BACKGROUND)
            .map_err(|e| LoadError::Background(e.to_string()))?;
        let rgba = if (img.width(), img.height()) == (QUOTE_WIDTH as u32, QUOTE_HEIGHT as u32) {
            img.to_rgba8()
        } else {
            img.resize_exact(1000, 1000, image::imageops::FilterType::Triangle)
                .to_rgba8()
        };
        let mut background = Pixmap::new(1000, 1000).unwrap();
        for (i, px) in rgba.pixels().enumerate() {
            let [r, g, b, a] = px.0;
            background.pixels_mut()[i] = ColorU8::from_rgba(r, g, b, a).premultiply();
        }
        Ok(Self {
            background,
            body_fonts: [
                load_font(LORA, "Lora-BoldItalic.ttf")?,
                load_font(OSWALD, "Oswald-SemiBold.ttf")?,
            ],
            watermark_font: load_font(MONTSERRAT, "Montserrat-ExtraBold.ttf")?,
        })
    }

    pub fn render(&self, text: &str, rng: &mut impl Rng) -> Vec<u8> {
        let mut pixmap = self.background.clone();

        let black_white = rng.random_range(0..=1) == 1;
        let alpha = if black_white { 0.5 } else { 1.0 };
        let c1 = random_color(alpha, rng);
        let c2 = random_color(alpha, rng);
        overlay_gradient(&mut pixmap, c1, c2);

        let font = &self.body_fonts[rng.random_range(0..self.body_fonts.len())];
        let size = QUOTE_WIDTH.min(QUOTE_HEIGHT) / 13.0;
        let stroke_width = size / 10.0;

        let scaled = font.as_scaled(PxScale::from(size));
        let mask = body_text_mask(text, &scaled);
        // White outline first (stamped around the glyphs), then the black fill.
        let outline_offsets = outline_offsets(stroke_width / 2.0);
        stamp(&mut pixmap, &mask, [1.0, 1.0, 1.0], &outline_offsets);
        stamp(&mut pixmap, &mask, [0.0, 0.0, 0.0], &[(0.0, 0.0)]);

        let wm_size = QUOTE_WIDTH.min(QUOTE_HEIGHT) / 25.0;
        let wm_scaled = self.watermark_font.as_scaled(PxScale::from(wm_size));
        let wm_mask = watermark_mask(&wm_scaled);
        stamp(&mut pixmap, &wm_mask, [0.0, 0.0, 0.0], &[(0.0, 0.0)]);

        pixmap.encode_png().unwrap_or_default()
    }
}

fn random_color(alpha: f32, rng: &mut impl Rng) -> [f32; 4] {
    [
        rng.random_range(0.2..=1.0),
        rng.random_range(0.2..=1.0),
        rng.random_range(0.2..=1.0),
        alpha,
    ]
}

/// Port of `image.fillGradient` with the diagonal gradient handles
/// (vec2(0, 0) -> vec2(w, h)), composited normally over the background.
fn overlay_gradient(pixmap: &mut Pixmap, c1: [f32; 4], c2: [f32; 4]) {
    let (w, h) = (QUOTE_WIDTH, QUOTE_HEIGHT);
    let d2 = w * w + h * h;
    let width = w as usize;
    for (i, px) in pixmap.pixels_mut().iter_mut().enumerate() {
        let x = (i % width) as f32;
        let y = (i / width) as f32;
        let t = ((x * w + y * h) / d2).clamp(0.0, 1.0);
        let a = c1[3] + (c2[3] - c1[3]) * t;
        let base = px.demultiply();
        let base_alpha = f32::from(base.alpha()) / 255.0;
        let out_alpha = a + base_alpha * (1.0 - a);
        let channel = |bc: f32, gc: f32| ((gc * a + bc * (1.0 - a)) * 255.0).round() as u8;
        let out = ColorU8::from_rgba(
            channel(f32::from(base.red()) / 255.0, c1[0] + (c2[0] - c1[0]) * t),
            channel(f32::from(base.green()) / 255.0, c1[1] + (c2[1] - c1[1]) * t),
            channel(f32::from(base.blue()) / 255.0, c1[2] + (c2[2] - c1[2]) * t),
            (out_alpha * 255.0).round() as u8,
        )
        .premultiply();
        *px = out;
    }
}

fn draw_glyph_to_mask(mask: &mut Mask, font: &FontRef<'static>, glyph: Glyph) {
    let Some(outline) = font.outline_glyph(glyph) else {
        return;
    };
    let min_x = outline.px_bounds().min.x.floor() as i32;
    let min_y = outline.px_bounds().min.y.floor() as i32;
    outline.draw(|x, y, alpha| {
        let cx = min_x + x as i32;
        let cy = min_y + y as i32;
        if cx >= 0 && cy >= 0 && cx < mask.width && cy < mask.height {
            let idx = (cy * mask.width + cx) as usize;
            mask.coverage[idx] = (mask.coverage[idx] + alpha).min(1.0);
        }
    });
}

fn measure_word(scaled: &PxScaleFont<&FontRef<'static>>, word: &str) -> f32 {
    word.chars()
        .map(|c| scaled.h_advance(scaled.glyph_id(c)))
        .sum()
}

/// Greedy word wrap inside the 900x900 box centered on the canvas
/// (`typeset(bounds = 0.9 * size, hAlign Center, vAlign Middle)`).
fn body_text_mask(text: &str, scaled: &PxScaleFont<&FontRef<'static>>) -> Mask {
    let mut mask = Mask::blank();
    let box_left = QUOTE_WIDTH * 0.05;
    let box_top = QUOTE_HEIGHT * 0.05;
    let box_width = QUOTE_WIDTH * 0.9;
    let box_height = QUOTE_HEIGHT * 0.9;

    let space_w = scaled.h_advance(scaled.glyph_id(' '));
    let line_height = scaled.height();
    let mut lines: Vec<(String, f32)> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0f32;
    for word in text.split(' ') {
        let word_w = measure_word(scaled, word);
        if word_w > box_width && !cur.is_empty() {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        if word_w > box_width {
            lines.push((word.to_owned(), word_w));
            continue;
        }
        if !cur.is_empty() && cur_w + space_w + word_w > box_width {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur = String::new();
            cur_w = 0.0;
        }
        if !cur.is_empty() {
            cur.push(' ');
            cur_w += space_w;
        }
        cur.push_str(word);
        cur_w += word_w;
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    if lines.is_empty() {
        return mask;
    }

    let total_height = lines.len() as f32 * line_height;
    let first_baseline = box_top + (box_height - total_height) / 2.0 + scaled.ascent();
    for (i, (line, line_w)) in lines.iter().enumerate() {
        let baseline = first_baseline + i as f32 * line_height;
        let mut pen_x = box_left + (box_width - line_w) / 2.0;
        for c in line.chars() {
            if c != ' ' {
                let glyph = font_glyph(scaled, c, pen_x, baseline);
                draw_glyph_to_mask(&mut mask, scaled.font, glyph);
            }
            pen_x += scaled.h_advance(scaled.glyph_id(c));
        }
    }
    mask
}

/// The watermark: `@MarkinimBot`, centered, near the bottom
/// (`translate(vec2(w / 2, h - size * 1.5))`, hAlign Center).
fn watermark_mask(scaled: &PxScaleFont<&FontRef<'static>>) -> Mask {
    let mut mask = Mask::blank();
    let text = "@MarkinimBot";
    let width = measure_word(scaled, text);
    let baseline = QUOTE_HEIGHT - scaled.scale.y * 1.5 + scaled.ascent();
    let mut pen_x = (QUOTE_WIDTH - width) / 2.0;
    for c in text.chars() {
        let glyph = font_glyph(scaled, c, pen_x, baseline);
        draw_glyph_to_mask(&mut mask, scaled.font, glyph);
        pen_x += scaled.h_advance(scaled.glyph_id(c));
    }
    mask
}

fn font_glyph(scaled: &PxScaleFont<&FontRef<'static>>, c: char, x: f32, y: f32) -> Glyph {
    scaled
        .font
        .glyph_id(c)
        .with_scale_and_position(scaled.scale, ab_glyph::point(x, y))
}

fn outline_offsets(radius: f32) -> Vec<(f32, f32)> {
    let mut offsets = vec![(0.0f32, 0.0f32)];
    for i in 0..16 {
        let angle = i as f32 * std::f32::consts::TAU / 16.0;
        offsets.push((angle.cos() * radius, angle.sin() * radius));
    }
    offsets
}

fn stamp(pixmap: &mut Pixmap, mask: &Mask, color: [f32; 3], offsets: &[(f32, f32)]) {
    for (dx, dy) in offsets {
        let (dx, dy) = (dx.round() as i32, dy.round() as i32);
        for y in 0..mask.height {
            for x in 0..mask.width {
                let alpha = mask.coverage[(y * mask.width + x) as usize];
                if alpha <= 0.0 {
                    continue;
                }
                let (px, py) = (x + dx, y + dy);
                if px < 0 || py < 0 || px >= mask.width || py >= mask.height {
                    continue;
                }
                blend_over(pixmap, px as usize, py as usize, color, alpha);
            }
        }
    }
}

fn blend_over(pixmap: &mut Pixmap, x: usize, y: usize, color: [f32; 3], alpha: f32) {
    let idx = y * (pixmap.width() as usize) + x;
    let dst = pixmap.pixels_mut()[idx].demultiply();
    let inv = 1.0 - alpha;
    let mix = |dc: u8, cc: f32| ((cc * alpha + f32::from(dc) / 255.0 * inv) * 255.0).round() as u8;
    let out = ColorU8::from_rgba(
        mix(dst.red(), color[0]),
        mix(dst.green(), color[1]),
        mix(dst.blue(), color[2]),
        out_alpha(dst.alpha(), alpha),
    )
    .premultiply();
    pixmap.pixels_mut()[idx] = out;
}

fn out_alpha(dst_alpha: u8, alpha: f32) -> u8 {
    let out = alpha + (f32::from(dst_alpha) / 255.0) * (1.0 - alpha);
    (out * 255.0).round() as u8
}
