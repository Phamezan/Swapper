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

/// The game's level-up box side and the ability slot pitch, measured at 1080p
/// (32px boxes, 44px apart) at the default HUD scale. The box is what the hint covers.
pub const BOX: f32 = 32.0 / 44.0;
const PITCH_1080: f32 = 44.0;
/// Where the centred HUD puts the arrows, in slot pitches: R's arrow this far left of
/// the client centre, and every arrow's bottom edge this far above the client bottom
/// (measured at 720p, 1050p and 1080p).
const R_LEFT: f32 = 0.04;
const BOTTOM: f32 = 2.33;
/// HUD scales the pitch may come from, relative to the default.
const HUD_SCALES: std::ops::RangeInclusive<f32> = 0.5..=2.0;

/// Centre-to-centre distance between ability slots, from arrows `found` over
/// `slots` (0 = Q, left to right); none with fewer than two arrows.
pub fn pitch(found: &[Arrow], slots: &[usize]) -> Option<f32> {
    let centre = |a: &Arrow| a.x as f32 + a.width as f32 / 2.0;
    let span = slots.last()?.checked_sub(*slots.first()?)?;
    (found.len() == slots.len() && span > 0).then(|| (centre(&found[found.len() - 1]) - centre(&found[0])) / span as f32)
}

/// The slot pitch when every arrow in `row` (client coordinates, for `slots`) sits where
/// the HUD puts the level-up arrows in a `width` x `height` client; none for gold elsewhere,
/// such as the buff icons above the ability bar. The pitch comes from the arrows' spacing,
/// else the `known` pitch measured earlier, else (one arrow, nothing known yet) the arrow's
/// height above the client bottom.
pub fn fit(row: &[Arrow], slots: &[usize], width: i32, height: i32, known: Option<f32>) -> Option<f32> {
    if row.is_empty() || row.len() != slots.len() {
        return None;
    }
    let bottom = |a: &Arrow| (a.y + a.height) as f32;
    let pitch = pitch(row, slots).or(known).unwrap_or((height as f32 - bottom(&row[0])) / BOTTOM);
    let default = PITCH_1080 * height as f32 / 1080.0;
    let near = |value: f32, at: f32| (value - at).abs() < pitch * 0.3;
    let placed = row.iter().zip(slots).all(|(a, &slot)| {
        near(a.x as f32 + a.width as f32 / 2.0, width as f32 / 2.0 - (3.0 - slot as f32 + R_LEFT) * pitch)
            && near(bottom(a), height as f32 - BOTTOM * pitch)
    });
    (placed && HUD_SCALES.contains(&(pitch / default))).then_some(pitch)
}

/// The game's level-up box around `arrow`, as client x, y, width, height. Sized
/// from the slot pitch: the arrow's lit pixels grow and shrink with its pulse.
/// The pulse lights the chevron from the bottom up, so its bottom edge and
/// horizontal centre stay put; the box centre sits 0.11 pitch above that edge.
pub fn level_box(arrow: Arrow, pitch: f32) -> [i32; 4] {
    let side = pitch * BOX;
    let (cx, cy) = (arrow.x as f32 + arrow.width as f32 / 2.0, (arrow.y + arrow.height) as f32 - 0.11 * pitch);
    [(cx - side / 2.0).round() as i32, (cy - side / 2.0).round() as i32, side.round() as i32, side.round() as i32]
}

/// `seen` widened to also cover `next` when both are the same arrow. The pulse
/// lights an uneven part of the chevron on each scan; together they span it all,
/// so the centre lands on the arrow instead of drifting with the lit side.
pub fn merge(seen: Arrow, next: Arrow, pitch: f32) -> Arrow {
    let centre = |a: Arrow| a.x as f32 + a.width as f32 / 2.0;
    let (x0, y0) = (seen.x.min(next.x), seen.y.min(next.y));
    let (x1, y1) = ((seen.x + seen.width).max(next.x + next.width), (seen.y + seen.height).max(next.y + next.height));
    let same = (centre(seen) - centre(next)).abs() < pitch / 2.0 && (x1 - x0) as f32 <= pitch * BOX;
    if same { Arrow { x: x0, y: y0, width: x1 - x0, height: y1 - y0 } } else { seen }
}

/// The level-up arrows' gold: bright red and green, little blue.
fn is_arrow_gold(pixel: u32) -> bool {
    let (r, g, b) = ((pixel >> 16) & 0xFF, (pixel >> 8) & 0xFF, pixel & 0xFF);
    r > 190 && g > 160 && b < 120 && r > b + 100
}

/// Every band of `expected` similar gold blobs in a BGRA strip, top to bottom. Rows with
/// gold are first split into bands at any vertical gap over 2px, so the arrows' band stays
/// apart from the ability icons under them; blobs are then split at column gaps.
/// A blob wider than `widest` is never an arrow.
pub fn rows_in(pixels: &[u32], width: usize, height: usize, expected: usize, widest: i32) -> Vec<Vec<Arrow>> {
    let mut rows = Vec::new();
    if expected == 0 || pixels.len() < width * height {
        return rows;
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
                && b.width <= widest
                && b.height >= 3
                && b.height * 2 <= height as i32
                && b.width * 2 >= b.height * 3
        };
        if row.len() == expected && row.iter().all(arrow) && alike(&row) {
            rows.push(row);
        }
    }
    rows
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

/// Captures the strip of the game client and returns the arrows for `slots` in client
/// coordinates, left to right, with the slot pitch; none unless all are found where
/// the HUD puts them for the `known` pitch.
pub fn find(bounds: Bounds, slots: &[usize], known: Option<f32>) -> Option<(Vec<Arrow>, f32)> {
    let (fx0, fy0, fx1, fy1) = STRIP;
    let x0 = (bounds.width as f32 * fx0) as i32;
    let y0 = (bounds.height as f32 * fy0) as i32;
    let width = (bounds.width as f32 * (fx1 - fx0)) as i32;
    let height = (bounds.height as f32 * (fy1 - fy0)) as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    let pixels = capture(bounds.x + x0, bounds.y + y0, width, height)?;
    let widest = (PITCH_1080 * bounds.height as f32 / 1080.0 * BOX * HUD_SCALES.end()).ceil() as i32;
    rows_in(&pixels, width as usize, height as usize, slots.len(), widest).into_iter().find_map(|row| {
        let row: Vec<Arrow> = row.into_iter().map(|a| Arrow { x: a.x + x0, y: a.y + y0, ..a }).collect();
        fit(&row, slots, bounds.width, bounds.height, known).map(|pitch| (row, pitch))
    })
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
        let found = &rows_in(&pixels, width, height, 3, 32)[0];
        assert_eq!(
            found.iter().map(|a| a.x).collect::<Vec<_>>(),
            vec![20, 60, 100]
        );
        assert_eq!((found[0].width, found[0].height), (16, 8));
        // A square icon (an item) above the arrows is never taken for a lone arrow.
        paint(&mut pixels, width, 120, 0, 6, 6);
        assert_eq!(rows_in(&pixels, width, height, 3, 32).len(), 1);
        assert!(rows_in(&pixels, width, height, 1, 32).is_empty());
        // A count that does not match any row finds nothing rather than guessing.
        assert!(rows_in(&pixels, width, height, 2, 32).is_empty());
        assert!(rows_in(&vec![DARK; width * height], width, height, 1, 32).is_empty());
    }

    #[test]
    fn box_follows_slot_pitch_not_lit_pixels() {
        let at = |x, width| Arrow { x, y: 100, width, height: 8 };
        // Q, W, R showing: R is two slots after W, so the pitch is 44 throughout.
        assert_eq!(pitch(&[at(10, 20), at(54, 20), at(142, 20)], &[0, 1, 3]), Some(44.0));
        assert_eq!(pitch(&[at(10, 20)], &[1]), None);
        assert_eq!(pitch(&[], &[0, 1]), None);
        // A dim pulse lights only part of the chevron; the box stays the same size.
        assert_eq!(level_box(at(374, 20), 44.0), [368, 87, 32, 32]);
        assert_eq!(level_box(Arrow { x: 380, y: 104, width: 8, height: 4 }, 44.0), [368, 87, 32, 32]);
    }

    /// Captured strips with W and E waiting, mid-pulse (1440p and 2160p are the
    /// 1080p capture upscaled), cut from the client at `origin`, and the game's W and E
    /// box centres measured in each (strip coordinates).
    #[test]
    fn real_frames_land_on_the_game_boxes_at_every_resolution() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/overlay/fixtures");
        for (name, client, origin, centres, middle) in [
            ("720p", (1280, 720), (320, 533), [260.0, 289.3], 116.0),
            ("1080p", (1920, 1080), (480, 799), [390.5, 434.5], 175.0),
            ("1440p", (2560, 1440), (640, 1065), [520.7, 579.3], 233.3),
            ("2160p", (3840, 2160), (960, 1598), [781.0, 869.0], 350.0),
        ] {
            let (pixels, width, strip) = super::super::capture::load_png(&dir.join(format!("arrows-{name}.png")));
            let default = PITCH_1080 * client.1 as f32 / 1080.0;
            let in_client = |row: &[Arrow]| row.iter().map(|a| Arrow { x: a.x + origin.0, y: a.y + origin.1, ..*a }).collect::<Vec<_>>();
            let rows = rows_in(&pixels, width, strip, 2, (default * BOX * 2.0).ceil() as i32);
            let found = rows.iter().find(|row| fit(&in_client(row), &[1, 2], client.0, client.1, None).is_some())
                .unwrap_or_else(|| panic!("{name}: {rows:?}"));
            let pitch = pitch(found, &[1, 2]).unwrap();
            assert!((pitch - default).abs() < default * 0.05, "{name}: pitch {pitch}");
            for (slot, (arrow, centre)) in (1..).zip(found.iter().zip(centres)) {
                let [x, y, side, _] = level_box(*arrow, pitch);
                let half = side as f32 / 2.0;
                assert!((x as f32 + half - centre).abs() <= 1.5, "{name}: {arrow:?}");
                assert!((y as f32 + half - middle).abs() <= 1.5, "{name}: {arrow:?}");
                // A lone arrow fits at its own slot only, with or without the pitch known.
                let lone = in_client(&[*arrow]);
                for known in [None, Some(pitch)] {
                    assert!(fit(&lone, &[slot], client.0, client.1, known).is_some(), "{name}: {arrow:?}");
                    assert!(fit(&lone, &[slot + 1], client.0, client.1, known).is_none(), "{name}");
                }
            }
            // Once the pitch is known, single blobs higher up (the buff icons) are never a lone arrow.
            for row in rows_in(&pixels, width, strip, 1, i32::MAX) {
                let above = row[0].y + row[0].height < found[0].y;
                let row = in_client(&row);
                assert!(!above || (0..4).all(|slot| fit(&row, &[slot], client.0, client.1, Some(pitch)).is_none()), "{name}: {row:?}");
            }
        }
    }

    #[test]
    fn gold_off_the_arrow_row_never_fits() {
        // 1080p: W's arrow bottom sits 2.33 pitches above the client bottom, 2.04 left of centre.
        let w = Arrow { x: 860, y: 969, width: 21, height: 10 };
        assert!((fit(&[w], &[1], 1920, 1080, None).unwrap() - 43.8).abs() < 0.5);
        assert_eq!(fit(&[w], &[1], 1920, 1080, Some(44.0)), Some(44.0));
        // The reported bug: a buff icon's gold patch a hundred pixels higher.
        for known in [None, Some(44.0)] {
            assert_eq!(fit(&[Arrow { y: 860, ..w }], &[1], 1920, 1080, known), None);
        }
        // Two buff blobs read as arrows would imply a huge pitch from their spacing.
        assert_eq!(fit(&[Arrow { x: 700, ..w }, Arrow { x: 900, ..w }], &[1, 2], 1920, 1080, None), None);
    }

    #[test]
    fn scans_of_a_pulsing_arrow_merge_to_its_centre() {
        let at = |x, width| Arrow { x, y: 100, width, height: 8 };
        // One scan lit only the right of the chevron, the next only the left.
        let both = merge(at(384, 10), at(374, 12), 44.0);
        assert_eq!(both, at(374, 20));
        assert_eq!(level_box(both, 44.0)[0], 368);
        // Another slot's arrow, or a blob wider than the box, is not merged.
        assert_eq!(merge(at(374, 20), at(418, 20), 44.0), at(374, 20));
        assert_eq!(merge(at(374, 20), at(360, 30), 44.0), at(374, 20));
    }

    #[test]
    fn only_bright_gold_counts_as_an_arrow() {
        assert!(is_arrow_gold(GOLD));
        assert!(!is_arrow_gold(0x00FF_FFFF), "white text such as LEVEL UP");
        assert!(!is_arrow_gold(0x0040_8040));
    }
}
