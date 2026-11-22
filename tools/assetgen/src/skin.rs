//! The game's windows and buttons: the pictures its skin draws them with
//! (`assets/skin/hexlords.tb.txt`), over Turbo Badger's default. A window is a
//! dark panel with a thin bronze border, round corners and a soft shadow;
//! its title bar darker, under a bronze line; a button a raised plate, and
//! the one a window means you to press (its first choice) in bronze.
//!
//! Each is a plate (`Plate`): a rounded rectangle with a border, filled top
//! to bottom from one color to another, on its shadow; drawn at the skin's
//! four sizes (96, 192, 288 and 384 dots an inch: `name.png`,
//! `name@192.png` ...), so it is sharp on any screen. The skin cuts each
//! into nine (`cut`): the corners stay, the edges and the middle stretch.

use crate::shapes::{Rgb, mix, segment};

/// A rounded plate: its size at 96 dpi (it is square; the skin stretches
/// it), the room around it for its shadow, its corners' radius, its
/// border's width and color, its fill from top to bottom, and how dark its
/// shadow is (0: none).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Plate {
    pub name: &'static str,
    pub size: u32,
    pub margin: f32,
    pub radius: f32,
    pub border: f32,
    pub border_color: Rgb,
    pub top: Rgb,
    pub bottom: Rgb,
    pub shadow: f32,
}

const fn hex(rgb: u32) -> Rgb {
    [((rgb >> 16) & 0xff) as f32 / 255.0, ((rgb >> 8) & 0xff) as f32 / 255.0, (rgb & 0xff) as f32 / 255.0]
}

/// The bronze of the borders, and the gold of the titles (the skin's text).
pub const BRONZE: Rgb = hex(0xa0_80_46);
pub const BRONZE_LIGHT: Rgb = hex(0xe0_c0_78);

pub const PLATES: [Plate; 6] = [
    // A window, a menu, the box over a unit: dark, a bronze line round it.
    Plate {
        name: "hg_window",
        size: 64,
        margin: 8.0,
        radius: 9.0,
        border: 2.0,
        border_color: BRONZE,
        top: hex(0x31_35_3c),
        bottom: hex(0x24_27_2c),
        shadow: 0.55,
    },
    // A window's title bar: darker, its corners a little round.
    Plate {
        name: "hg_title",
        size: 32,
        margin: 0.0,
        radius: 5.0,
        border: 1.0,
        border_color: hex(0x55_48_30),
        top: hex(0x1f_22_26),
        bottom: hex(0x17_19_1c),
        shadow: 0.0,
    },
    // A button: a plate a little lighter than the window, raised.
    Plate {
        name: "hg_button",
        size: 32,
        margin: 1.0,
        radius: 5.0,
        border: 1.0,
        border_color: hex(0x6a_70_7a),
        top: hex(0x4f_55_5e),
        bottom: hex(0x3a_3e_46),
        shadow: 0.35,
    },
    // Pressed, or the chosen one of a group: sunk, a bronze line round it.
    Plate {
        name: "hg_button_pressed",
        size: 32,
        margin: 1.0,
        radius: 5.0,
        border: 1.0,
        border_color: BRONZE,
        top: hex(0x26_29_2e),
        bottom: hex(0x32_36_3d),
        shadow: 0.0,
    },
    // The button a window means you to press: bronze.
    Plate {
        name: "hg_primary",
        size: 32,
        margin: 1.0,
        radius: 5.0,
        border: 1.0,
        border_color: BRONZE_LIGHT,
        top: hex(0xc4_9e_52),
        bottom: hex(0x96_74_34),
        shadow: 0.35,
    },
    Plate {
        name: "hg_primary_pressed",
        size: 32,
        margin: 1.0,
        radius: 5.0,
        border: 1.0,
        border_color: BRONZE_LIGHT,
        top: hex(0x8a_6a_2e),
        bottom: hex(0xa8_84_42),
        shadow: 0.0,
    },
];

/// A window's close button: a light cross on nothing (the title bar under
/// it), outlined dark so it shows on any title; pressed, in bronze. Its
/// size at 96 dpi, and its names.
pub const CLOSE_SIZE: u32 = 20;
pub const CLOSE_NAMES: [&str; 2] = ["hg_close", "hg_close_pressed"];

/// The close cross's color and opacity at a point, in its pixels at 96 dpi.
pub fn close_ink(x: f32, y: f32, pressed: bool) -> (Rgb, f32) {
    let s = CLOSE_SIZE as f32;
    let (a, b) = (s * 0.3, s * 0.7);
    let d = segment(x, y, a, a, b, b).min(segment(x, y, b, a, a, b));
    let (body, line) = (1.4, 2.6);
    if d <= body {
        (if pressed { BRONZE_LIGHT } else { hex(0xdc_d6_c8) }, 1.0)
    } else if d <= line {
        (hex(0x10_11_13), 0.9)
    } else {
        ([0.0; 3], 0.0)
    }
}

/// The close cross's picture at `scale` (`pressed`: the bronze one).
pub fn close_picture(pressed: bool, scale: u32) -> (usize, Vec<u8>) {
    let n = (CLOSE_SIZE * scale) as usize;
    let mut out = vec![0u8; n * n * 4];
    let samples = 4;
    for py in 0..n {
        for px in 0..n {
            let (mut sum, mut alpha) = ([0f32; 3], 0f32);
            for sy in 0..samples {
                for sx in 0..samples {
                    let x = (px as f32 + (sx as f32 + 0.5) / samples as f32) / scale as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / samples as f32) / scale as f32;
                    let (color, a) = close_ink(x, y, pressed);
                    for (total, part) in sum.iter_mut().zip(color) {
                        *total += part * a;
                    }
                    alpha += a;
                }
            }
            let count = (samples * samples) as f32;
            let o = (py * n + px) * 4;
            let color = if alpha > 0.0 { sum.map(|t| t / alpha) } else { [0.0; 3] };
            for (byte, part) in out[o..o + 3].iter_mut().zip(color) {
                *byte = (part.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            out[o + 3] = ((alpha / count).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    (n, out)
}

/// A name at a scale ("hg_close@192.png").
pub fn scaled_name(name: &str, scale: u32) -> String {
    if scale == 1 { format!("{name}.png") } else { format!("{name}@{}.png", 96 * scale) }
}

/// The sizes the skin has pictures for, as a scale of 96 dpi; and the name
/// of a plate's picture at a scale ("hg_window.png", "hg_window@192.png").
pub const SCALES: [u32; 4] = [1, 2, 3, 4];

pub fn file_name(plate: &Plate, scale: u32) -> String {
    scaled_name(plate.name, scale)
}

/// How far a point is from the edge of a rounded rectangle of `half` (its
/// half width and height) and `radius`, centred on 0: below zero inside.
fn rounded(x: f32, y: f32, half: f32, radius: f32) -> f32 {
    let (qx, qy) = (x.abs() - (half - radius), y.abs() - (half - radius));
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

impl Plate {
    /// Its color and opacity at a point, in its own pixels at 96 dpi from
    /// its top left (a pixel's middle is at .5).
    pub fn ink(&self, x: f32, y: f32) -> (Rgb, f32) {
        let size = self.size as f32;
        let half = size / 2.0 - self.margin;
        let (cx, cy) = (x - size / 2.0, y - size / 2.0);
        let d = rounded(cx, cy, half, self.radius);
        if d > 0.0 {
            // Its shadow, a little below it, fading out.
            if self.margin <= 0.0 || self.shadow <= 0.0 {
                return ([0.0; 3], 0.0);
            }
            let ds = rounded(cx, cy - self.margin * 0.25, half, self.radius);
            let t = (1.0 - ds.max(0.0) / self.margin).clamp(0.0, 1.0);
            return ([0.0; 3], self.shadow * t * t);
        }
        let t = ((y - self.margin) / (size - 2.0 * self.margin)).clamp(0.0, 1.0);
        let fill = mix(self.top, self.bottom, t);
        if d > -self.border { (self.border_color, 1.0) } else { (fill, 1.0) }
    }

    /// Its picture at `scale` (1 to 4): RGBA, a pixel at a time, each the
    /// mean of 16 samples within it.
    pub fn picture(&self, scale: u32) -> (usize, Vec<u8>) {
        let n = (self.size * scale) as usize;
        let mut out = vec![0u8; n * n * 4];
        let samples = 4;
        for py in 0..n {
            for px in 0..n {
                let (mut sum, mut alpha) = ([0f32; 3], 0f32);
                for sy in 0..samples {
                    for sx in 0..samples {
                        let x = (px as f32 + (sx as f32 + 0.5) / samples as f32) / scale as f32;
                        let y = (py as f32 + (sy as f32 + 0.5) / samples as f32) / scale as f32;
                        let (color, a) = self.ink(x, y);
                        for (total, part) in sum.iter_mut().zip(color) {
                            *total += part * a;
                        }
                        alpha += a;
                    }
                }
                let count = (samples * samples) as f32;
                let o = (py * n + px) * 4;
                let color = if alpha > 0.0 { sum.map(|t| t / alpha) } else { [0.0; 3] };
                for (byte, part) in out[o..o + 3].iter_mut().zip(color) {
                    *byte = (part.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                out[o + 3] = ((alpha / count).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        (n, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate(name: &str) -> Plate {
        *PLATES.iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn a_window_is_dark_inside_a_bronze_line_on_a_shadow() {
        let w = plate("hg_window");
        // The middle: the fill; just inside its edge: the border; outside,
        // the shadow, and nothing at the very edge of the picture.
        let (fill, a) = w.ink(32.0, 32.0);
        assert_eq!(a, 1.0);
        assert!(fill.iter().all(|c| *c < 0.25), "{fill:?}");
        assert_eq!(w.ink(32.0, w.margin + 0.5), (BRONZE, 1.0));
        let (_, shadow) = w.ink(32.0, w.size as f32 - w.margin + 2.0);
        assert!(shadow > 0.05 && shadow <= w.shadow, "{shadow}");
        assert_eq!(w.ink(0.2, 0.2).1, 0.0);
        // A corner is round: its very corner is outside.
        assert!(w.ink(w.margin + 0.5, w.margin + 0.5).1 < 1.0);
    }

    #[test]
    fn its_pictures_are_the_skins_four_sizes_and_the_same_at_each() {
        let b = plate("hg_button");
        for scale in SCALES {
            let (n, rgba) = b.picture(scale);
            assert_eq!(n, 32 * scale as usize);
            assert_eq!(rgba.len(), n * n * 4);
            // The middle pixel is the fill, opaque, the same at every size.
            let o = ((n / 2) * n + n / 2) * 4;
            assert_eq!(rgba[o + 3], 255);
            let (_, one) = b.picture(1);
            let m = (16 * 32 + 16) * 4;
            for c in 0..3 {
                assert!((i32::from(rgba[o + c]) - i32::from(one[m + c])).abs() <= 3);
            }
        }
        assert_eq!(file_name(&b, 1), "hg_button.png");
        assert_eq!(file_name(&b, 3), "hg_button@288.png");
    }

    #[test]
    fn the_close_cross_is_light_with_a_dark_line_and_bronze_when_pressed() {
        let c = CLOSE_SIZE as f32 / 2.0;
        let (light, a) = close_ink(c, c, false);
        assert_eq!(a, 1.0);
        assert!(light.iter().all(|v| *v > 0.75), "{light:?}");
        assert_eq!(close_ink(c, c, true).0, BRONZE_LIGHT);
        assert_eq!(close_ink(1.0, 1.0, false).1, 0.0, "nothing in the corner");
        assert_eq!(close_ink(c, c - 2.0, false).0, hex(0x10_11_13), "the dark line beside the cross");
    }

    #[test]
    fn the_primary_button_is_bronze_and_lighter_than_the_others() {
        let light = |p: &Plate| {
            let (c, _) = p.ink(16.0, 16.0);
            c.iter().sum::<f32>()
        };
        assert!(light(&plate("hg_primary")) > light(&plate("hg_button")) + 0.5);
        assert!(light(&plate("hg_button")) > light(&plate("hg_button_pressed")));
    }
}
