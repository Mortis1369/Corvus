//! make-icons: turns the one branding source into every icon the two programs embed.
//!
//! Input (first found wins):  `branding/karasu.svg`, else `branding/karasu.png`.
//! Output in `branding/generated/`:
//!   karasu.ico                 16 / 32 / 48 / 256 px (256 stored as PNG inside the .ico)
//!   karasu-{16..512}.png       square PNGs, light-grey glyph on transparent
//!
//! The source is a dark glyph on white (the current PNG) or on transparent (the future SVG).
//! Either way the output is normalised to: glyph colour (230,230,230), alpha from the glyph,
//! trimmed to its bounding box, padded 10% and squared. Drop in the real SVG and re-run.
//!
//! Usage: `cargo run -p make-icons [-- <branding-dir>]`

use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

const PNG_SIZES: &[u32] = &[16, 32, 48, 64, 128, 256, 512];
const ICO_SIZES: &[u32] = &[16, 32, 48, 256];
const GLYPH_RGB: [u8; 3] = [230, 230, 230];
const PAD: f32 = 0.10;
const RENDER_SIDE: u32 = 1024;

fn main() {
    if let Err(e) = run() {
        eprintln!("make-icons: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let branding = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("branding"));
    let out = branding.join("generated");
    std::fs::create_dir_all(&out)?;

    let svg = branding.join("karasu.svg");
    let png = branding.join("karasu.png");
    let (src, raw) = if svg.is_file() {
        (svg.clone(), render_svg(&svg)?)
    } else if png.is_file() {
        (png.clone(), image::open(&png)?.to_rgba8())
    } else {
        return Err(format!(
            "no source: expected {} or {}",
            svg.display(),
            png.display()
        )
        .into());
    };
    println!("source: {} ({}x{})", src.display(), raw.width(), raw.height());

    let glyph = to_glyph(&raw);
    let square = trim_and_square(&glyph, PAD)
        .ok_or("source image is blank after thresholding")?;
    println!("glyph bbox squared to {}x{}", square.width(), square.height());

    for &n in PNG_SIZES {
        let p = out.join(format!("karasu-{n}.png"));
        resize(&square, n).save(&p)?;
        println!("wrote {}", p.display());
    }

    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for &n in ICO_SIZES {
        let img = resize(&square, n);
        let icon = ico::IconImage::from_rgba_data(n, n, img.into_raw());
        let entry = if n >= 256 {
            ico::IconDirEntry::encode_as_png(&icon)?
        } else {
            ico::IconDirEntry::encode(&icon)?
        };
        dir.add_entry(entry);
    }
    let ico_path = out.join("karasu.ico");
    dir.write(std::fs::File::create(&ico_path)?)?;
    println!("wrote {}", ico_path.display());
    Ok(())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn render_svg(path: &Path) -> Result<RgbaImage, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default())?;
    let size = tree.size();
    let scale = RENDER_SIDE as f32 / size.width().max(size.height());
    let w = (size.width() * scale).ceil() as u32;
    let h = (size.height() * scale).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("pixmap alloc failed")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia gives premultiplied RGBA; un-premultiply.
    let mut img = RgbaImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let p = pixmap.pixel(x, y).ok_or("pixel out of range")?;
        let a = p.alpha();
        let un = |c: u8| {
            if a == 0 {
                0
            } else {
                ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8
            }
        };
        *px = Rgba([un(p.red()), un(p.green()), un(p.blue()), a]);
    }
    Ok(img)
}

/// Normalise any source to "light glyph on transparent".
/// If the source has real transparency, the glyph is wherever alpha is; otherwise (opaque
/// image, e.g. black ink on white paper) the glyph is wherever the pixel is dark.
fn to_glyph(src: &RgbaImage) -> RgbaImage {
    let has_alpha = src.pixels().any(|p| p[3] < 250);
    let mut out = RgbaImage::new(src.width(), src.height());
    for (x, y, p) in src.enumerate_pixels() {
        let alpha = if has_alpha {
            p[3]
        } else {
            let lum = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).round();
            255 - lum.clamp(0.0, 255.0) as u8
        };
        out.put_pixel(x, y, Rgba([GLYPH_RGB[0], GLYPH_RGB[1], GLYPH_RGB[2], alpha]));
    }
    out
}

/// Crop to the alpha bounding box, then pad to a square with `pad` of the glyph side on each edge.
fn trim_and_square(img: &RgbaImage, pad: f32) -> Option<RgbaImage> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 8 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        return None;
    }
    let w = x1 - x0 + 1;
    let h = y1 - y0 + 1;
    let glyph_side = w.max(h);
    let side = (glyph_side as f32 * (1.0 + 2.0 * pad)).ceil() as u32;
    let mut out = RgbaImage::from_pixel(side, side, Rgba([GLYPH_RGB[0], GLYPH_RGB[1], GLYPH_RGB[2], 0]));
    let ox = (side - w) / 2;
    let oy = (side - h) / 2;
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(ox + x, oy + y, *img.get_pixel(x0 + x, y0 + y));
        }
    }
    Some(out)
}

fn resize(img: &RgbaImage, n: u32) -> RgbaImage {
    image::imageops::resize(img, n, n, FilterType::Lanczos3)
}
