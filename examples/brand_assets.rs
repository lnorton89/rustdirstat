// ============================================================================
// Module:       brand_assets (example)
// Description:  Re-renders the PNGs in assets/brand/ from the shared mark
//               definition, so the project's art cannot drift from the app's.
//
// Dependencies: rustdirstat::brand; image (PNG encoding); ab_glyph and egui's
//               bundled Ubuntu-Light for the wordmark
// ============================================================================

//! Regenerates `assets/brand/` — run it with:
//!
//! ```sh
//! cargo run --example brand_assets
//! ```
//!
//! The icons are [`rustdirstat::brand::rgba`] at a range of sizes, which
//! is the same call the window icon is built from, so the README art and
//! the taskbar icon are the same drawing by construction rather than by
//! anyone remembering to re-export one when the other changes.
//!
//! The wordmark is set in Ubuntu-Light — not because the banner needs a
//! typeface of its own, but because that is what egui's default
//! proportional family resolves to, so the name on the project page is
//! the name in the app's own title bar. The font bytes are read back out
//! of `egui::FontDefinitions::default()` rather than vendored, for the
//! same reason.
//!
//! An example rather than a test: it writes into the source tree, which
//! is not something `cargo test` should ever do. It is still built by
//! `cargo clippy --all-targets`, so it cannot rot silently.

use std::path::Path;

use ab_glyph::{Font, FontRef, Glyph, Point, PxScale, ScaleFont};
use anyhow::{anyhow, Context, Result};
use rustdirstat::brand;

/// Where the generated art lands, relative to the crate root.
const OUT_DIR: &str = "assets/brand";

/// Icon sizes to emit. 512 is for stores and READMEs, 256/128 for
/// desktop shells, 64/32 for the small end where the mark has to still
/// resolve into four tiles rather than a smear.
const ICON_SIZES: [u32; 5] = [512, 256, 128, 64, 32];

/// The sizes packed into `icon.ico`, which is what Explorer, the taskbar
/// and Task Manager read.
///
/// Windows picks the nearest of these per surface rather than scaling
/// one, so a 16px taskbar entry gets a mark drawn for 16px instead of a
/// 256px one squeezed down to mush. 256 is the format's maximum.
const ICO_SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];

/// The banner's canvas. 2× the width it is displayed at in the README,
/// so it stays sharp on a HiDPI screen.
const BANNER: (u32, u32) = (1440, 380);

/// The ink the banner is drawn on. Dark, because the mark's own colours
/// are mid-tone and a white field leaves the frame ring washed out.
const BANNER_BG: [u8; 3] = [18, 21, 28];
const BANNER_EDGE: [u8; 3] = [38, 44, 57];
const BANNER_TITLE: [u8; 3] = [239, 242, 247];
const BANNER_TAGLINE: [u8; 3] = [148, 160, 178];

const TITLE: &str = "RustDirStat";
const TAGLINE: &str = "Where did my disk go?";
const SUBLINE: &str =
    "A WinDirStat clone in Rust — native GUI and terminal UI over one scanning core.";

fn main() -> Result<()> {
    let out = Path::new(OUT_DIR);
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    for size in ICON_SIZES {
        let path = out.join(format!("icon-{size}.png"));
        write_png(&path, size, size, &brand::rgba(size as usize))?;
        println!("wrote {}", path.display());
    }

    let path = out.join("icon.ico");
    write_ico(&path)?;
    println!("wrote {}", path.display());

    let path = out.join("banner.png");
    let (width, height) = BANNER;
    write_png(&path, width, height, &banner()?)?;
    println!("wrote {}", path.display());

    let path = out.join("wordmark.png");
    let (pixels, width, height) = wordmark()?;
    write_png(&path, width, height, &pixels)?;
    println!("wrote {}", path.display());

    Ok(())
}

/// The README header: the mark, the name, and one line of what it is.
fn banner() -> Result<Vec<u8>> {
    let (width, height) = BANNER;
    let mut canvas = Canvas::new(width, height);
    canvas.fill_rounded(BANNER_BG, 255, 28.0);
    canvas.stroke_rounded(BANNER_EDGE, 28.0, 2.0);

    let font = ui_font()?;
    let font = FontRef::try_from_slice(&font).map_err(|e| anyhow!("reading the UI font: {e}"))?;

    // One lockup, not art with captions beside it: the mark and the
    // three lines of copy share a centre line, and every line starts at
    // the same left edge so the text reads as a single column.
    let mark = 220_u32;
    let left = 80_i64;
    // Centred on the block, not on the mark: the title's cap line sits
    // above the mark's top edge, so centring the mark alone leaves the
    // whole lockup riding high in the card.
    let top = (height as i64 - mark as i64) / 2 + 13;
    canvas.blit(&brand::rgba(mark as usize), mark, mark, left, top);

    let text_left = left + mark as i64 + 52;
    canvas.text(&font, TITLE, text_left, top - 26, 100.0, BANNER_TITLE);
    canvas.text(&font, TAGLINE, text_left, top + 94, 44.0, BANNER_TAGLINE);
    canvas.text(&font, SUBLINE, text_left, top + 158, 30.0, BANNER_TAGLINE);

    Ok(canvas.pixels)
}

/// The mark and the name on a transparent field, for anywhere the banner
/// is too heavy — a docs header, a release page, a slide.
fn wordmark() -> Result<(Vec<u8>, u32, u32)> {
    let (width, height) = (900_u32, 220_u32);
    let mut canvas = Canvas::new(width, height);

    let font = ui_font()?;
    let font = FontRef::try_from_slice(&font).map_err(|e| anyhow!("reading the UI font: {e}"))?;

    let mark = 160_u32;
    let top = (height - mark) as i64 / 2;
    canvas.blit(&brand::rgba(mark as usize), mark, mark, 24, top);
    canvas.text(
        &font,
        TITLE,
        24 + mark as i64 + 40,
        top + 34,
        92.0,
        BANNER_TITLE,
    );

    Ok((canvas.pixels, width, height))
}

/// egui's default proportional font, as bytes.
///
/// Pulled from the live `FontDefinitions` rather than a vendored copy so
/// that if egui ever changes what "proportional" means, the banner
/// changes with the app instead of silently disagreeing with it.
fn ui_font() -> Result<Vec<u8>> {
    let definitions = egui::FontDefinitions::default();
    let family = definitions
        .families
        .get(&egui::FontFamily::Proportional)
        .and_then(|names| names.first())
        .ok_or_else(|| anyhow!("egui has no proportional font family"))?;
    let data = definitions
        .font_data
        .get(family)
        .ok_or_else(|| anyhow!("egui's proportional family names a font it does not carry"))?;
    Ok(data.font.to_vec())
}

/// An RGBA8 drawing surface, with just the handful of operations the
/// brand art needs. Not a general-purpose rasteriser: it exists so this
/// example does not pull a 2D graphics stack in behind it.
struct Canvas {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: vec![0; (width * height * 4) as usize],
            width,
            height,
        }
    }

    /// Alpha-blends `color` at `coverage` (0..=1) over one pixel.
    fn blend(&mut self, x: i64, y: i64, color: [u8; 3], coverage: f32) {
        if coverage <= 0.0 || x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let coverage = coverage.min(1.0);
        let offset = ((y as u32 * self.width + x as u32) * 4) as usize;
        let existing = self.pixels[offset + 3] as f32 / 255.0;
        let alpha = coverage + existing * (1.0 - coverage);
        if alpha <= 0.0 {
            return;
        }
        for (channel, &over) in color.iter().enumerate() {
            let under = self.pixels[offset + channel] as f32;
            self.pixels[offset + channel] =
                ((over as f32 * coverage + under * existing * (1.0 - coverage)) / alpha) as u8;
        }
        self.pixels[offset + 3] = (alpha * 255.0) as u8;
    }

    /// Fills the whole canvas as a rounded rectangle.
    fn fill_rounded(&mut self, color: [u8; 3], alpha: u8, radius: f32) {
        let coverage = alpha as f32 / 255.0;
        for y in 0..self.height as i64 {
            for x in 0..self.width as i64 {
                let inside = self.corner_coverage(x as f32 + 0.5, y as f32 + 0.5, radius);
                self.blend(x, y, color, inside * coverage);
            }
        }
    }

    /// Draws the rounded rectangle's edge, `width` pixels thick inward.
    fn stroke_rounded(&mut self, color: [u8; 3], radius: f32, width: f32) {
        for y in 0..self.height as i64 {
            for x in 0..self.width as i64 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let outer = self.corner_coverage(px, py, radius);
                let inner = self.inset_coverage(px, py, radius, width);
                self.blend(x, y, color, (outer - inner).max(0.0));
            }
        }
    }

    /// How much of a point lies inside the canvas' rounded outline.
    fn corner_coverage(&self, x: f32, y: f32, radius: f32) -> f32 {
        self.inset_coverage(x, y, radius, 0.0)
    }

    /// The same, for an outline pulled `inset` pixels inward — which is
    /// how the border is drawn without a second geometry path.
    fn inset_coverage(&self, x: f32, y: f32, radius: f32, inset: f32) -> f32 {
        let (left, top) = (inset, inset);
        let right = self.width as f32 - inset;
        let bottom = self.height as f32 - inset;
        if x < left || y < top || x > right || y > bottom {
            return 0.0;
        }
        let radius = (radius - inset).max(0.0);
        let dx = (left + radius - x).max(x - (right - radius)).max(0.0);
        let dy = (top + radius - y).max(y - (bottom - radius)).max(0.0);
        if dx == 0.0 || dy == 0.0 {
            return 1.0;
        }
        // Soften the arc over one pixel so the corner is not stair-stepped.
        let distance = (dx * dx + dy * dy).sqrt();
        (radius + 0.5 - distance).clamp(0.0, 1.0)
    }

    /// Composites an RGBA source over the canvas at `(left, top)`.
    fn blit(&mut self, source: &[u8], width: u32, height: u32, left: i64, top: i64) {
        for y in 0..height as i64 {
            for x in 0..width as i64 {
                let offset = ((y as u32 * width + x as u32) * 4) as usize;
                let Some(pixel) = source.get(offset..offset + 4) else {
                    continue;
                };
                let color = [pixel[0], pixel[1], pixel[2]];
                self.blend(left + x, top + y, color, pixel[3] as f32 / 255.0);
            }
        }
    }

    /// Lays out `text` on one line with `top` as its cap line, and
    /// rasterises it. Kerning is applied pair by pair; without it the
    /// wordmark's "rD" and "St" sit visibly loose.
    fn text(&mut self, font: &FontRef, text: &str, left: i64, top: i64, size: f32, color: [u8; 3]) {
        let scaled = font.as_scaled(PxScale::from(size));
        let mut caret = left as f32;
        let baseline = top as f32 + scaled.ascent();
        let mut previous: Option<ab_glyph::GlyphId> = None;

        for character in text.chars() {
            let id = scaled.glyph_id(character);
            if let Some(previous) = previous {
                caret += scaled.kern(previous, id);
            }
            let glyph: Glyph = id.with_scale_and_position(
                size,
                Point {
                    x: caret,
                    y: baseline,
                },
            );
            caret += scaled.h_advance(id);
            previous = Some(id);

            let Some(outline) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|x, y, coverage| {
                self.blend(
                    bounds.min.x as i64 + x as i64,
                    bounds.min.y as i64 + y as i64,
                    color,
                    coverage,
                );
            });
        }
    }
}

/// Packs the mark, at every size Windows asks for, into one `.ico`.
///
/// Assembled here rather than through an encoder crate because the
/// container is trivial and the frames are already PNGs: since Vista an
/// icon directory may hold PNG-encoded entries directly, so this is a
/// header, one 16-byte record per size, and the PNG bytes themselves.
/// That keeps `brand.rs` the only place the mark is defined — the whole
/// reason these assets are generated rather than drawn.
fn write_ico(path: &Path) -> Result<()> {
    let frames: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .map(|&size| Ok((size, png_bytes(size)?)))
        .collect::<Result<_>>()?;

    let count = u16::try_from(frames.len()).context("too many icon sizes for one .ico")?;
    let mut out = Vec::new();
    out.extend_from_slice(&0_u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1_u16.to_le_bytes()); // 1 = icon, 2 = cursor
    out.extend_from_slice(&count.to_le_bytes());

    // Entries come first, so every offset has to account for all of them.
    let mut offset = 6 + 16 * u32::from(count);
    for (size, png) in &frames {
        let length = u32::try_from(png.len()).context("icon frame too large for the format")?;
        // 256 is stored as 0: the field is one byte and 256 does not fit.
        let dimension = u8::try_from(*size).unwrap_or(0);
        out.push(dimension); // width
        out.push(dimension); // height
        out.push(0); // palette size, 0 for truecolour
        out.push(0); // reserved
        out.extend_from_slice(&1_u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32_u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += length;
    }
    for (_size, png) in &frames {
        out.extend_from_slice(png);
    }

    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

/// The mark at `size`, PNG-encoded in memory.
fn png_bytes(size: u32) -> Result<Vec<u8>> {
    let pixels = brand::rgba(size as usize);
    let buffer = image::RgbaImage::from_raw(size, size, pixels)
        .ok_or_else(|| anyhow!("pixels do not fill {size}x{size}"))?;
    let mut encoded = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut encoded, image::ImageFormat::Png)
        .with_context(|| format!("encoding the {size}px icon"))?;
    Ok(encoded.into_inner())
}

/// Encodes straight RGBA8 to a PNG on disk.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
    let buffer = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| anyhow!("{} pixels do not fill {width}x{height}", pixels.len()))?;
    buffer
        .save_with_format(path, image::ImageFormat::Png)
        .with_context(|| format!("writing {}", path.display()))
}
