//! The window icon, drawn in code (no image decoder needed): the green
//! rounded square of `assets/icon.svg` with a white crescent.

/// Icon edge length in pixels.
pub const ICON_SIZE: u32 = 64;

/// Icon background colour (`#16a34a`, as in `assets/icon.svg`).
const GREEN: [u8; 3] = [0x16, 0xa3, 0x4a];

/// Supersampling factor per axis for antialiasing.
const SS: u32 = 4;

/// The icon as eframe icon data.
pub fn app_icon() -> egui::IconData {
    egui::IconData {
        rgba: icon_rgba(ICON_SIZE),
        width: ICON_SIZE,
        height: ICON_SIZE,
    }
}

/// Draw the icon as unmultiplied RGBA8, `size × size` pixels.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = 14.0 / 64.0 * s; // corner radius, as in the SVG
    let moon_c = (0.5 * s, 0.5 * s);
    let moon_r = 0.32 * s;
    let shadow_c = (0.62 * s, 0.42 * s);
    let shadow_r = 0.28 * s;

    let inside_square = |x: f32, y: f32| {
        let cx = x.clamp(radius, s - radius);
        let cy = y.clamp(radius, s - radius);
        (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
    };
    let in_circle =
        |x: f32, y: f32, c: (f32, f32), r: f32| (x - c.0).powi(2) + (y - c.1).powi(2) <= r * r;

    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut bg, mut moon) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = px as f32 + (sx as f32 + 0.5) / SS as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
                    if inside_square(x, y) {
                        bg += 1;
                        if in_circle(x, y, moon_c, moon_r) && !in_circle(x, y, shadow_c, shadow_r) {
                            moon += 1;
                        }
                    }
                }
            }
            let n = (SS * SS) as f32;
            let moon_frac = moon as f32 / bg.max(1) as f32;
            let mix = |c: u8| (f32::from(c) * (1.0 - moon_frac) + 255.0 * moon_frac).round() as u8;
            rgba.extend_from_slice(&[
                mix(GREEN[0]),
                mix(GREEN[1]),
                mix(GREEN[2]),
                (bg as f32 / n * 255.0).round() as u8,
            ]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_transparent_corners_and_a_white_crescent() {
        let px = icon_rgba(ICON_SIZE);
        assert_eq!(px.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let at = |x: u32, y: u32| {
            let i = ((y * ICON_SIZE + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        assert_eq!(at(0, 0)[3], 0);
        // Lower-left limb of the crescent is lit; the edge is green.
        assert_eq!(at(16, 40), [255, 255, 255, 255]);
        assert_eq!(at(2, 32), [GREEN[0], GREEN[1], GREEN[2], 255]);
    }
}
