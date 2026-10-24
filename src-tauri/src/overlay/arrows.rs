//! Finds the game's own level-up arrows above the ability bar, so the overlay
//! can highlight the right one at any resolution and HUD scale. It reads a
//! strip of the screen (like any capture tool); the game process is never
//! touched, and the overlay window is excluded from capture.
use super::capture::capture;

use super::render::Bounds;

/// A box in game-client pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrow {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The searched strip, as fractions of the client: the ability bar's band
/// above the gold counter, across the HUD's middle.
const STRIP: (f32, f32, f32, f32) = (0.25, 0.74, 0.75, 0.96);

/// The level-up arrows' gold: bright red and green, little blue.
fn is_arrow_gold(pixel: u32) -> bool {
    let (r, g, b) = ((pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF);
    r > 190 && g > 160 && b < 120 && r > b + 100
}

/// The topmost band of `expected` similar gold blobs in a BGRA strip. Rows with
/// gold are first split into bands at any vertical gap over 2px, so the arrows' band stays
/// apart from the ability icons under them; blobs are then split at column gaps.
pub fn find_in(pixels: &[u32], width: usize, height: usize, expected: usize) -> Vec<Arrow> {
    if expected == 0 || pixels.len() < width * height {
        return Vec::new();
    }
    // An arrow's two arms are a few pixels apart; arrows are tens of pixels apart.
    let join = (height / 25).max(3);
    let gold = |x: usize, y: usize| is_arrow_gold(pixels[y * width + x]);
    let lit: Vec<bool> = (0..height)
        .map(|y| (0..width).any(|x| gold(x, y)))
        .collect();
    // The arrows are the same shape; a band of mixed sizes (spells, items) is not them.
    let alike = |row: &[Arrow]| {
        let (min, max) = row.iter().fold((i32::MAX, 0), |(lo, hi), b| {
            (lo.min(b.width), hi.max(b.width))
        });
        max * 10 <= min * 14
    };
    for (top, bottom) in spans(&lit, 2) {
        let columns: Vec<bool> = (0..width)
            .map(|x| (top..=bottom).any(|y| gold(x, y)))
            .collect();
        let row: Vec<Arrow> = spans(&columns, join)
            .into_iter()
            .map(|(x0, x1)| {
                let ys: Vec<usize> = (top..=bottom)
                    .filter(|&y| (x0..=x1).any(|x| gold(x, y)))
                    .collect();
                Arrow {
                    x: x0 as i32,
                    y: ys[0] as i32,
                    width: (x1 - x0 + 1) as i32,
                    height: (ys[ys.len() - 1] - ys[0] + 1) as i32,
                }
            })
            // Specks of gold-ish noise are not shapes at all.
            .filter(|b| b.width >= 4 || b.height >= 4)
            .collect();
        // Every shape in the band must look like an arrow: a band with an icon or
        // text in it is some other part of the HUD.
        let arrow = |b: &Arrow| {
            b.width >= 6
                && b.height >= 3
                && b.height * 2 <= height as i32
                && b.width * 2 >= b.height * 3
        };
        if row.len() == expected && row.iter().all(arrow) && alike(&row) {
            return row;
        }
    }
    Vec::new()
}

/// Runs of `true` as (first, last) indices, joining runs separated by at most `join` gaps.
fn spans(on: &[bool], join: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, _) in on.iter().enumerate().filter(|(_, lit)| **lit) {
        match out.last_mut() {
            Some(last) if i <= last.1 + join + 1 => last.1 = i,
            _ => out.push((i, i)),
        }
    }
    out
}

/// Captures the strip of the game client and returns `expected` arrows in
/// client coordinates, left to right, or nothing when they are not all found.
pub fn find(bounds: Bounds, expected: usize) -> Vec<Arrow> {
    let (fx0, fy0, fx1, fy1) = STRIP;
    let x0 = (bounds.width as f32 * fx0) as i32;
    let y0 = (bounds.height as f32 * fy0) as i32;
    let width = (bounds.width as f32 * (fx1 - fx0)) as i32;
    let height = (bounds.height as f32 * (fy1 - fy0)) as i32;
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let Some(pixels) = capture(bounds.x + x0, bounds.y + y0, width, height) else {
        return Vec::new();
    };
    find_in(&pixels, width as usize, height as usize, expected)
        .into_iter()
        .map(|a| Arrow {
            x: a.x + x0,
            y: a.y + y0,
            ..a
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLD: u32 = 0x00E8_C84A;
    const DARK: u32 = 0x0010_1418;

    fn paint(pixels: &mut [u32], width: usize, x: usize, y: usize, w: usize, h: usize) {
        for row in y..y + h {
            for col in x..x + w {
                pixels[row * width + col] = GOLD;
            }
        }
    }

    #[test]
    fn finds_the_arrow_row_and_ignores_gold_below_it() {
        let (width, height) = (200, 60);
        let mut pixels = vec![DARK; width * height];
        // Three arrows in one row, then a Flash-like gold icon lower down.
        // Each arrow as two arms 3px apart, like the game's chevrons.
        for x in [20, 60, 100] {
            paint(&mut pixels, width, x, 10, 7, 8);
            paint(&mut pixels, width, x + 9, 10, 7, 8);
        }
        paint(&mut pixels, width, 150, 40, 14, 14);
        // A mixed-size row (a spell icon and an item) never passes as arrows.
        paint(&mut pixels, width, 20, 40, 30, 10);
        paint(&mut pixels, width, 60, 40, 10, 10);
        let found = find_in(&pixels, width, height, 3);
        assert_eq!(
            found.iter().map(|a| a.x).collect::<Vec<_>>(),
            vec![20, 60, 100]
        );
        assert_eq!((found[0].width, found[0].height), (16, 8));
        // A square icon (an item) above the arrows is never taken for a lone arrow.
        paint(&mut pixels, width, 120, 0, 6, 6);
        assert_eq!(find_in(&pixels, width, height, 3).len(), 3);
        assert!(find_in(&pixels, width, height, 1).is_empty());
        // A count that does not match any row finds nothing rather than guessing.
        assert!(find_in(&pixels, width, height, 2).is_empty());
        assert!(find_in(&vec![DARK; width * height], width, height, 1).is_empty());
    }

    #[test]
    fn only_bright_gold_counts_as_an_arrow() {
        assert!(is_arrow_gold(GOLD));
        assert!(!is_arrow_gold(0x00FF_FFFF), "white text such as LEVEL UP");
        assert!(!is_arrow_gold(0x0040_8040));
    }
}
