//! Locates the Tab frame in client pixels; never supplies a fallback position.
use std::time::{Duration, Instant};
use super::{capture::capture, render::Bounds};

pub const FRAME_WIDTH: f32 = 1156.0;
pub const FRAME_HEIGHT: f32 = 465.0;
pub const HEADER: f32 = 73.0;
pub const ROW: f32 = 76.0;

fn rgb(pixel: u32) -> (u32, u32, u32) {
    ((pixel >> 16) & 255, (pixel >> 8) & 255, pixel & 255)
}

fn top_gold(pixel: u32) -> bool {
    let (r, g, b) = rgb(pixel);
    (106..230).contains(&r) && (81..200).contains(&g) && (36..140).contains(&b)
        && r > g && g > b + 20
}

fn edge_gold(pixel: u32) -> bool {
    let (r, g, b) = rgb(pixel);
    r > 35 && g > 25 && b < 140 && r > g + 6 && g > b + 12
}

fn divider_gold(pixel: u32) -> bool {
    let (r, g, b) = rgb(pixel);
    (35..85).contains(&r) && (30..75).contains(&g) && (10..45).contains(&b)
        && r > g + 4 && g > b + 12
}

/// Sparse checks on both top halves, both sides, bottom and centre divider.
fn verify_in(pixels: &[u32], width: usize, height: usize, rect: Bounds) -> bool {
    if rect.x < 0 || rect.y < 0 || rect.width <= 0 || rect.height <= 0
        || rect.x as usize + rect.width as usize > width
        || rect.y as usize + rect.height as usize > height
        || width.checked_mul(height).is_none_or(|len| pixels.len() < len)
    { return false; }
    let s = rect.width as f32 / FRAME_WIDTH;
    let radius = (3.0 * s).ceil() as i32;
    let near = |x: f32, y: f32, test: fn(u32) -> bool| {
        let (x, y) = (rect.x + x.round() as i32, rect.y + y.round() as i32);
        (y - radius..=y + radius).any(|py| {
            (x - radius..=x + radius).any(|px| {
                px >= rect.x && py >= rect.y && px < rect.x + rect.width && py < rect.y + rect.height
                    && test(pixels[py as usize * width + px as usize])
            })
        })
    };
    [0.08, 0.2, 0.32, 0.68, 0.8, 0.92].into_iter()
        .all(|f| near(rect.width as f32 * f, 1.0 * s, top_gold))
        && [0.12, 0.35, 0.58, 0.8, 0.95].into_iter().all(|f| {
            let y = rect.height as f32 * f;
            near(1.0 * s, y, edge_gold) && near(rect.width as f32 - 2.0 * s, y, edge_gold)
        })
        && [0.2, 0.4, 0.6, 0.8, 0.96].into_iter()
            .all(|f| near(rect.width as f32 / 2.0, rect.height as f32 * f, divider_gold))
        && [0.1, 0.3, 0.7, 0.9].into_iter()
            .all(|f| near(rect.width as f32 * f, rect.height as f32 - 2.0 * s, edge_gold))
}

/// Linear search for a long top-border run, confirmed by the rest of the frame.
/// Geometry scales with client height; position is always measured from pixels.
pub fn find_in(pixels: &[u32], width: usize, height: usize) -> Option<Bounds> {
    let length = width.checked_mul(height)?;
    if width == 0 || height == 0 || pixels.len() < length
        || width > i32::MAX as usize || height > i32::MAX as usize { return None; }
    let s = height as f32 / 1080.0;
    let (fw, fh) = ((FRAME_WIDTH * s).round() as i32, (FRAME_HEIGHT * s).round() as i32);
    if fw <= 0 || fh <= 0 || fw as usize > width || fh as usize > height { return None; }
    for (y, row) in pixels[..length].chunks_exact(width).take(height - fh as usize + 1).enumerate() {
        let mut start = None;
        for (x, pixel) in row.iter().copied().chain(Some(0)).enumerate() {
            if top_gold(pixel) {
                start.get_or_insert(x);
            } else if let Some(left) = start.take() {
                let run = (x - left) as f32;
                if run < 400.0 * s || run > 560.0 * s { continue; }
                let rect = Bounds { x: left as i32 - (20.0 * s).round() as i32, y: y as i32, width: fw, height: fh };
                if verify_in(pixels, width, height, rect) { return Some(rect); }
            }
        }
    }
    None
}

pub struct Tracker {
    client: Option<Bounds>,
    frame: Option<Bounds>,
    next_check: Instant,
}

impl Default for Tracker {
    fn default() -> Self {
        Self { client: None, frame: None, next_check: Instant::now() }
    }
}

impl Tracker {
    pub fn update(&mut self, client: Option<Bounds>, held: bool, now: Instant) -> Option<Bounds> {
        self.update_with(client, held, now, |r| capture(r.x, r.y, r.width, r.height))
    }

    fn update_with(&mut self, client: Option<Bounds>, held: bool, now: Instant,
        mut read: impl FnMut(Bounds) -> Option<Vec<u32>>) -> Option<Bounds>
    {
        let Some(client) = client.filter(|_| held) else {
            self.client = None;
            self.frame = None;
            self.next_check = now;
            return None;
        };
        if self.client != Some(client) {
            self.client = Some(client);
            self.frame = None;
            self.next_check = now;
        }
        if now < self.next_check { return self.frame; }
        if let Some(frame) = self.frame {
            let region = Bounds { x: client.x + frame.x, y: client.y + frame.y, ..frame };
            let verified = read(region).is_some_and(|pixels| verify_in(&pixels,
                frame.width as usize, frame.height as usize, Bounds { x: 0, y: 0, ..frame }));
            if verified {
                self.next_check = now + Duration::from_secs(1);
                return Some(frame);
            }
        }
        self.frame = read(client).and_then(|pixels| find_in(&pixels, client.width as usize, client.height as usize));
        // Tab may be sampled before the game draws it; absent frames retry at 4 Hz.
        self.next_check = now + if self.frame.is_some() { Duration::from_secs(1) } else { Duration::from_millis(250) };
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> (Vec<u32>, usize, usize) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("src/overlay/fixtures/scoreboard-{name}-frame.png"));
        super::super::capture::load_png(&path)
    }

    fn crop(pixels: &[u32], width: usize, rect: Bounds) -> Vec<u32> {
        (rect.y..rect.y + rect.height).flat_map(|y| {
            let start = y as usize * width + rect.x as usize;
            pixels[start..start + rect.width as usize].iter().copied()
        }).collect()
    }

    #[test]
    fn recorded_frames_are_found_at_both_dragged_positions() {
        for (name, x, y) in [("full", 380, 235), ("practice", 368, 148)] {
            let (pixels, width, height) = fixture(name);
            let rect = find_in(&pixels, width, height).expect("recorded scoreboard frame");
            assert!((rect.x - x).abs() <= 2 && (rect.y - y).abs() <= 2, "{rect:?}");
            assert!((rect.width - 1156).abs() <= 2 && (rect.height - 465).abs() <= 2);
        }
    }

    #[test]
    fn frame_geometry_scales_with_client_height() {
        let (pixels, width, height) = fixture("full");
        for (w, h) in [(1280, 720), (2560, 1440)] {
            let scaled: Vec<u32> = (0..h).flat_map(|y| {
                let pixels = &pixels;
                (0..w).map(move |x| pixels[y * height / h * width + x * width / w])
            }).collect();
            let rect = find_in(&scaled, w, h).expect("scaled frame");
            let s = h as f32 / height as f32;
            assert!((rect.x as f32 - 380.0 * s).abs() <= 3.0, "{rect:?}");
            assert!((rect.y as f32 - 235.0 * s).abs() <= 3.0, "{rect:?}");
        }
    }

    #[test]
    fn top_border_alone_or_a_missing_divider_is_not_a_scoreboard() {
        let (mut pixels, width, height) = fixture("full");
        for row in pixels.chunks_exact_mut(width).skip(300) {
            row[953..963].fill(0);
        }
        assert_eq!(find_in(&pixels, width, height), None);
        pixels[240 * width..].fill(0);
        assert_eq!(find_in(&pixels, width, height), None);
    }

    #[test]
    fn tracker_checks_once_a_second_follows_dragging_and_resets_on_release() {
        let (pixels, width, height) = fixture("full");
        let (moved, _, _) = fixture("practice");
        let client = Bounds { x: -1920, y: 120, width: width as i32, height: height as i32 };
        let mut tracker = Tracker::default();
        let now = Instant::now();
        let mut requests = Vec::new();
        let mut read = |r: Bounds| {
            requests.push(r);
            Some(crop(&pixels, width, Bounds { x: r.x - client.x, y: r.y - client.y, ..r }))
        };
        let frame = tracker.update_with(Some(client), true, now, &mut read).unwrap();
        assert_eq!(tracker.update_with(Some(client), true, now + Duration::from_millis(999), &mut read), Some(frame));
        assert_eq!(tracker.update_with(Some(client), true, now + Duration::from_secs(1), &mut read), Some(frame));
        assert_eq!(requests.len(), 2, "one full search, then one cropped verification");
        assert_eq!(requests[0], client);
        assert_eq!(requests[1], Bounds { x: client.x + frame.x, y: client.y + frame.y, ..frame });
        let mut drag_requests = 0;
        let dragged = tracker.update_with(Some(client), true, now + Duration::from_secs(2), |r| {
            drag_requests += 1;
            Some(crop(&moved, width, Bounds { x: r.x - client.x, y: r.y - client.y, ..r }))
        }).unwrap();
        assert_eq!(drag_requests, 2, "failed cached verification triggers a full search");
        assert_eq!((dragged.x, dragged.y), (368, 148));
        assert_eq!(tracker.update_with(Some(client), false, now, |_| panic!("no capture on release")), None);
        assert_eq!(tracker.update_with(None, true, now, |_| panic!("no capture when unfocused")), None);
        assert_eq!(tracker.update_with(Some(client), true, now, |_| Some(pixels.clone())), Some(frame));
    }

    #[test]
    fn tracker_retries_a_late_frame_and_drops_a_closed_or_failed_capture() {
        let (pixels, width, height) = fixture("full");
        let client = Bounds { x: 0, y: 0, width: width as i32, height: height as i32 };
        let now = Instant::now();
        let mut tracker = Tracker::default();
        assert_eq!(tracker.update_with(Some(client), true, now, |_| None), None);
        assert_eq!(tracker.update_with(Some(client), true, now + Duration::from_millis(249), |_| panic!("throttled")), None);
        assert!(tracker.update_with(Some(client), true, now + Duration::from_millis(250), |_| Some(pixels.clone())).is_some());
        assert_eq!(tracker.update_with(Some(client), true, now + Duration::from_millis(1250), |_| None), None);
        assert_eq!(tracker.update_with(Some(Bounds { width: 1280, height: 720, ..client }), true, now, |_| None), None);
    }

    #[test]
    #[ignore = "opt-in native capture and pixel-search timing; no screenshots saved"]
    fn detection_cost() {
        let (pixels, width, height) = fixture("full");
        let frame = find_in(&pixels, width, height).unwrap();
        let cached = crop(&pixels, width, frame);
        let now = Instant::now();
        for _ in 0..30 {
            assert_eq!(find_in(std::hint::black_box(&pixels), width, height), Some(frame));
        }
        println!("full fixture search mean: {:?}", now.elapsed() / 30);
        let now = Instant::now();
        for _ in 0..1000 {
            assert!(verify_in(std::hint::black_box(&cached), frame.width as usize, frame.height as usize, Bounds { x: 0, y: 0, ..frame }));
        }
        println!("cached pixel verification mean: {:?}", now.elapsed() / 1000);
        for (label, w, h) in [("full client", width as i32, height as i32), ("cached frame", frame.width, frame.height)] {
            let now = Instant::now();
            for _ in 0..5 { assert!(capture(0, 0, w, h).is_some()); }
            println!("{label} GDI capture mean: {:?}", now.elapsed() / 5);
        }
    }

    #[test]
    fn no_frame_or_invalid_pixels_never_supply_a_position() {
        assert_eq!(find_in(&vec![0x00101820; 1920 * 1080], 1920, 1080), None);
        assert_eq!(find_in(&[], 1920, 1080), None);
        assert_eq!(find_in(&[], 0, 0), None);
        assert_eq!(find_in(&[], usize::MAX, 2), None);
    }
}
