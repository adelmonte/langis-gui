//! Blurhashes (https://blurha.sh): a few DCT components in base 83, drawn
//! blurred in place of a photo or video until it's downloaded, as Desktop
//! does. Decoded once at 32 pixels and kept as a texture.

use std::collections::HashMap;
use std::sync::Mutex;

use egui::{Context, TextureHandle, TextureOptions};

const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

fn base83(s: &[u8]) -> Option<u32> {
    s.iter().try_fold(0u32, |acc, c| Some(acc * 83 + CHARS.iter().position(|x| x == c)? as u32))
}

fn to_linear(v: u32) -> f32 {
    let v = v as f32 / 255.;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(v: f32) -> u8 {
    let v = v.clamp(0., 1.);
    let s = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1. / 2.4) - 0.055 };
    (s * 255. + 0.5) as u8
}

fn sign_pow(v: f32, e: f32) -> f32 {
    v.abs().powf(e).copysign(v)
}

/// Decodes `hash` to `w` by `h` RGBA pixels, or `None` if it isn't one.
pub fn decode(hash: &str, w: usize, h: usize) -> Option<Vec<u8>> {
    let b = hash.as_bytes();
    if b.len() < 6 {
        return None;
    }
    let size = base83(&b[0..1])?;
    let (nx, ny) = ((size % 9 + 1) as usize, (size / 9 + 1) as usize);
    if b.len() != 4 + 2 * nx * ny {
        return None;
    }
    let max_ac = (base83(&b[1..2])? as f32 + 1.) / 166.;
    let mut colors = Vec::with_capacity(nx * ny);
    let dc = base83(&b[2..6])?;
    colors.push([to_linear(dc >> 16), to_linear((dc >> 8) & 255), to_linear(dc & 255)]);
    for i in 1..nx * ny {
        let v = base83(&b[4 + i * 2..6 + i * 2])?;
        let q = |x: u32| sign_pow((x as f32 - 9.) / 9., 2.) * max_ac;
        colors.push([q(v / (19 * 19)), q((v / 19) % 19), q(v % 19)]);
    }
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut c = [0f32; 3];
            for j in 0..ny {
                for i in 0..nx {
                    let basis = (std::f32::consts::PI * x as f32 * i as f32 / w as f32).cos() * (std::f32::consts::PI * y as f32 * j as f32 / h as f32).cos();
                    let k = colors[i + j * nx];
                    c[0] += k[0] * basis;
                    c[1] += k[1] * basis;
                    c[2] += k[2] * basis;
                }
            }
            let at = (x + y * w) * 4;
            out[at..at + 4].copy_from_slice(&[to_srgb(c[0]), to_srgb(c[1]), to_srgb(c[2]), 255]);
        }
    }
    Some(out)
}

static CACHE: Mutex<Option<HashMap<String, Option<TextureHandle>>>> = Mutex::new(None);

/// The blurhash as a texture, decoded the first time.
pub fn texture(ctx: &Context, hash: &str) -> Option<TextureHandle> {
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(Default::default);
    if cache.len() > 500 {
        cache.clear();
    }
    cache
        .entry(hash.to_owned())
        .or_insert_with(|| {
            let pixels = decode(hash, 32, 32)?;
            let image = egui::ColorImage::from_rgba_unmultiplied([32, 32], &pixels);
            Some(ctx.load_texture(format!("blurhash:{hash}"), image, TextureOptions::LINEAR))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_the_reference_hash() {
        // The example on blurha.sh.
        let pixels = super::decode("LEHV6nWB2yk8pyo0adR*.7kCMdnj", 4, 3).expect("valid");
        assert_eq!(pixels.len(), 4 * 3 * 4);
        assert!(pixels.chunks(4).all(|p| p[3] == 255));
        // Not one: wrong length, bad characters.
        assert!(super::decode("LEHV6nWB2yk8", 4, 3).is_none());
        assert!(super::decode("!!!!!!", 4, 3).is_none());
    }
}
