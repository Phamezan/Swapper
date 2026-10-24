use super::super::model::{DisplayData, Gold, Objective};
use super::{icons, Surface, PULSE_STEPS};
use super::super::scoreboard::{FRAME_HEIGHT, HEADER, ROW};
use windows_sys::Win32::Graphics::GdiPlus::*;

// ARGB palette, matched to Swapper's dark theme and DPM's widget colours.
const PANEL: u32 = 0xE6101522;
const EDGE: u32 = 0x33FFFFFF;
const TRACK: u32 = 0x40FFFFFF;
const TEXT: u32 = 0xFFE9ECF4;
const MUTED: u32 = 0xFF8B93A7;
const BLUE: u32 = 0xFF6E9BFF;
/// League's gold (the client's `#C8AA6E`).
const COIN: u32 = 0xFFC8AA6E;
const RED: u32 = 0xFFFF6B7A;
/// Swapper's signature green (`--brand-green`) on graphite, for the level-up hint.
const BRAND_GREEN: u32 = 0xFFC5E39E;
const HINT_FILL: u32 = 0xF2181A17;

/// Scoreboard text sizes at 1080p, scaled with the frame.
const ROW_TEXT: f32 = 14.0;
const TOTAL_TEXT: f32 = 19.0;
/// The empty header corners, outside the first and last team stat icons.
const CORNER: f32 = 120.0;
/// Dark edge around scoreboard text and arrows.
pub(super) const OUTLINE: u32 = 0xF0080B10;

struct GoldMarker {
    number: RectF,
    bar: Option<RectF>,
    gap: i64,
}

fn gold_markers(frame: RectF, gold: Gold) -> Vec<GoldMarker> {
    let s = frame.Height / FRAME_HEIGHT;
    let centre = frame.X + frame.Width / 2.0;
    gold.rows.into_iter().enumerate().filter_map(|(i, gap)| {
        let gap = gap?;
        let top = frame.Y + (HEADER + i as f32 * ROW) * s;
        Some(GoldMarker {
            // Centred on the row's stretch of the divider; the bar breaks around it.
            number: RectF { X: centre - 40.0 * s, Y: top + (ROW / 2.0 - 9.0) * s, Width: 80.0 * s, Height: 18.0 * s },
            bar: (gap != 0).then_some(RectF { X: centre - 1.5 * s, Y: top + 16.0 * s, Width: 3.0 * s, Height: (ROW - 20.0) * s }),
            gap,
        })
    }).collect()
}

/// Each team's total gold in its own top corner of the frame, beside its stat icons.
fn team_totals(frame: RectF, gold: Gold) -> [(RectF, i64, u32); 2] {
    let s = frame.Height / FRAME_HEIGHT;
    let corner = |x: f32| RectF { X: x, Y: frame.Y + 22.0 * s, Width: CORNER * s, Height: 28.0 * s };
    [(corner(frame.X + 10.0 * s), gold.blue, BLUE), (corner(frame.X + frame.Width - (10.0 + CORNER) * s), gold.red, RED)]
}

impl Surface {
    /// Only coloured divider segments and outlined numbers, on the detected Tab frame.
    pub(super) fn gold(&self, rect: RectF, gold: Gold) -> Result<(), String> {
        let s = rect.Height / FRAME_HEIGHT;
        for (corner, total, color) in team_totals(rect, gold) {
            let glyphs = self.outlined(corner, &short(total, false), TOTAL_TEXT * s, color)?;
            // League's gold coins just before the number, sized to its cap height.
            let size = glyphs.Height * 1.55;
            let coin = RectF { X: glyphs.X - size - 5.0 * s, Y: glyphs.Y + (glyphs.Height - size) / 2.0, Width: size, Height: size };
            self.icon(coin, &icons::GOLD, COIN)?;
        }
        for marker in gold_markers(rect, gold) {
            let color = match marker.gap.cmp(&0) {
                std::cmp::Ordering::Greater => BLUE,
                std::cmp::Ordering::Less => RED,
                std::cmp::Ordering::Equal => MUTED,
            };
            if let Some(bar) = marker.bar {
                let (gap_top, gap_bottom) = (marker.number.Y - 1.0 * s, marker.number.Y + marker.number.Height + 1.0 * s);
                for (from, to) in [(bar.Y, gap_top), (gap_bottom, bar.Y + bar.Height)] {
                    if to > from {
                        self.with_brush(color, |brush| unsafe { GdipFillRectangle(self.graphics, brush, bar.X, from, bar.Width, to - from) })?;
                    }
                }
            }
            let glyphs = self.outlined(marker.number, &short(marker.gap.saturating_abs(), false), ROW_TEXT * s, color)?;
            if marker.gap != 0 {
                self.lead_arrow(glyphs, marker.gap > 0, ROW_TEXT * s, color)?;
            }
        }
        std::result::Result::Ok(())
    }

    /// Pill with a countdown ring that fills as the objective approaches spawn.
    pub(super) fn objective(&self, rect: RectF, objective: Objective) -> Result<(), String> {
        let s = self.scale;
        self.rounded(rect, rect.Height / 2.0, PANEL, Some((EDGE, 1.0 * s)))?;
        let color = match objective.label {
            "Baron" => 0xFFB678FF,
            "Elder" => 0xFF9FE7FF,
            _ => 0xFFFF9466,
        };
        let ring = rect.Height - 10.0 * s;
        let ring_rect = RectF {
            X: rect.X + 5.0 * s,
            Y: rect.Y + 5.0 * s,
            Width: ring,
            Height: ring,
        };
        self.arc(ring_rect, 360.0, TRACK, 2.5 * s)?;
        self.arc(ring_rect, 360.0 * elapsed(objective), color, 2.5 * s)?;
        let inset = ring * 0.12;
        let mask = match objective.label {
            "Baron" => &icons::BARON,
            "Elder" => &icons::ELDER,
            _ => &icons::DRAGON,
        };
        self.icon(
            RectF {
                X: ring_rect.X + inset,
                Y: ring_rect.Y + inset,
                Width: ring - 2.0 * inset,
                Height: ring - 2.0 * inset,
            },
            mask,
            color,
        )?;
        let up = objective.seconds == 0;
        let label = if up {
            "UP".to_string()
        } else {
            format!("{}:{:02}", objective.seconds / 60, objective.seconds % 60)
        };
        self.text(
            RectF {
                X: ring_rect.X + ring,
                Width: rect.Width - ring - 10.0 * s,
                ..rect
            },
            &label,
            self.medium,
            if up { color } else { TEXT },
        )
    }

    /// Covers the game's level-up arrow for the recommended skill with a graphite
    /// box and a green double chevron, with three diamonds above
    /// lighting up in a rising wave. Sized from the detected arrow, so it
    /// follows the HUD scale.
    pub(super) fn arrow(&self, rect: RectF, pulse: u8) -> Result<(), String> {
        let unit = rect.Width.max(rect.Height * 2.0);
        let side = unit * 1.3;
        let (cx, cy) = (rect.X + rect.Width / 2.0, rect.Y + rect.Height / 2.0);
        let frame = RectF {
            X: cx - side / 2.0,
            Y: cy - side / 2.0,
            Width: side,
            Height: side,
        };
        self.rounded(
            frame,
            unit * 0.12,
            HINT_FILL,
            Some((BRAND_GREEN, (unit * 0.08).max(1.5))),
        )?;
        let (half, rise, gap) = (unit * 0.3, unit * 0.2, unit * 0.22);
        for top in [cy - (rise + gap) / 2.0, cy - (rise + gap) / 2.0 + gap] {
            let points = [
                PointF {
                    X: cx - half,
                    Y: top + rise,
                },
                PointF { X: cx, Y: top },
                PointF {
                    X: cx + half,
                    Y: top + rise,
                },
            ];
            self.with_pen(BRAND_GREEN, (unit * 0.11).max(1.5), |pen| unsafe {
                GdipSetPenStartCap(pen, LineCapRound);
                GdipSetPenEndCap(pen, LineCapRound);
                GdipSetPenLineJoin(pen, LineJoinRound);
                GdipDrawLines(self.graphics, pen, points.as_ptr(), 3)
            })?;
        }
        let half = unit * 0.1;
        for i in 0..3u8 {
            let y = frame.Y - unit * 0.28 - f32::from(i) * unit * 0.3;
            self.diamond(
                cx,
                y,
                half,
                (pulse_alpha(pulse, i) << 24) | (BRAND_GREEN & 0x00FF_FFFF),
            )?;
        }
        std::result::Result::Ok(())
    }
}

pub(super) fn scale(height: i32) -> f32 {
    (height as f32 / 1080.0).clamp(0.65, 2.0)
}

fn elapsed(objective: Objective) -> f32 {
    if objective.period == 0 {
        return 1.0;
    }
    (1.0 - objective.seconds as f32 / objective.period as f32).clamp(0.0, 1.0)
}

/// `19.6K` / `+2.1K` / `-700`, like DPM's gold widget.
fn short(value: i64, signed: bool) -> String {
    let sign = if signed && value >= 0 { "+" } else { "" };
    if value.abs() >= 1000 {
        format!("{sign}{:.1}K", value as f64 / 1000.0)
    } else {
        format!("{sign}{value}")
    }
}

pub(super) enum Widget {
    Gold(RectF, Gold),
    Objective(RectF, Objective),
    Arrow(RectF, u8),
}

pub(super) fn widgets(width: i32, height: i32, data: &DisplayData) -> Vec<Widget> {
    let s = scale(height);
    let (w, h) = (width as f32, height as f32);
    let mut widgets = Vec::new();
    if let (Some(gold), Some(frame)) = (data.gold, data.scoreboard) {
        widgets.push(Widget::Gold(
            RectF { X: frame.x as f32, Y: frame.y as f32, Width: frame.width as f32, Height: frame.height as f32 },
            gold,
        ));
    }
    let (pill, gap) = (112.0 * s, 8.0 * s);
    let start = w - data.objectives.len() as f32 * (pill + gap) - 6.0 * s;
    for (i, objective) in data.objectives.iter().enumerate() {
        widgets.push(Widget::Objective(
            RectF {
                X: start + i as f32 * (pill + gap),
                Y: h * 0.048,
                Width: pill,
                Height: 36.0 * s,
            },
            *objective,
        ));
    }
    if let (Some(_), Some([x, y, aw, ah])) = (data.skill, data.arrow) {
        widgets.push(Widget::Arrow(
            RectF {
                X: x as f32,
                Y: y as f32,
                Width: aw as f32,
                Height: ah as f32,
            },
            data.pulse,
        ));
    }
    widgets
}

/// Diamond `index` (0 = lowest) brightness at `pulse`: a wave rising from the
/// box, each diamond lit a few steps after the one below, dim between waves.
fn pulse_alpha(pulse: u8, index: u8) -> u32 {
    let step = (pulse + PULSE_STEPS - index * 3) % PULSE_STEPS;
    let lit = 1.0 - (f32::from(step) / 6.0 - 1.0).abs();
    (0x40 as f32 + lit.max(0.0) * (0xFF - 0x40) as f32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DisplayData {
        DisplayData {
            gold: Some(Gold {
                blue: 19_600,
                red: 17_500,
                rows: [Some(1_600), Some(-4_200), Some(2_400), None, Some(0)],
            }),
            scoreboard: None,
            objectives: vec![
                Objective {
                    label: "Dragon",
                    seconds: 101,
                    period: 300,
                },
                Objective {
                    label: "Baron",
                    seconds: 0,
                    period: 360,
                },
            ],
            skill: Some('R'),
            arrow: Some([60, 120, 20, 8]),
            pulse: 0,
        }
    }

    #[test]
    fn gold_is_absent_until_the_scoreboard_is_detected() {
        assert!(widgets(1920, 1080, &sample()).iter().all(|w| !matches!(w, Widget::Gold(..))));
    }

    #[test]
    fn diamonds_light_up_bottom_first_and_never_vanish() {
        assert_eq!(pulse_alpha(6, 0), 0xFF);
        assert_eq!(pulse_alpha(9, 1), 0xFF);
        assert!(pulse_alpha(6, 0) > pulse_alpha(6, 2));
        assert!((0..PULSE_STEPS).all(|p| (0..3).all(|i| pulse_alpha(p, i) >= 0x40)));
    }

    #[test]
    fn layouts_stay_inside_client_at_common_resolutions() {
        for (width, height) in [
            (360, 240),
            (1280, 720),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
        ] {
            let widgets = widgets(width, height, &sample());
            assert_eq!(widgets.len(), 3);
            assert_eq!(
                widgets
                    .iter()
                    .filter(|w| matches!(w, Widget::Arrow(..)))
                    .count(),
                1
            );
            for widget in widgets {
                let (Widget::Gold(rect, _) | Widget::Objective(rect, _) | Widget::Arrow(rect, _)) =
                    widget;
                assert!(rect.X >= 0.0 && rect.Y >= 0.0);
                assert!(
                    rect.X + rect.Width <= width as f32 && rect.Y + rect.Height <= height as f32
                );
            }
        }
        assert!(widgets(1920, 1080, &DisplayData::default()).is_empty());
    }

    #[test]
    fn markers_follow_the_divider_and_skip_unknown_rows() {
        let gold = sample().gold.unwrap();
        let frame = RectF { X: 380.0, Y: 235.0, Width: 1156.0, Height: 465.0 };
        let totals = team_totals(frame, gold);
        assert_eq!((totals[0].1, totals[1].1), (gold.blue, gold.red));
        assert!(totals[0].0.X > frame.X && totals[1].0.X + totals[1].0.Width < frame.X + frame.Width, "totals stay inside the frame corners");
        let mut markers = gold_markers(frame, gold);
        assert_eq!(markers.len(), 4, "four known rows");
        // Keep the old indexes below: slot 0 used to be the team gap.
        markers.insert(0, GoldMarker { number: markers[0].number, bar: None, gap: 0 });
        assert_eq!(markers[1].gap, 1600);
        assert_eq!(markers[1].bar.as_ref().unwrap().Width, 3.0);
        assert_eq!(markers[2].gap, -4200);
        assert_eq!(markers[4].gap, 0);
        assert!(markers[4].bar.is_none(), "even rows have only a muted zero");
        for marker in &markers {
            assert_eq!(marker.number.X + marker.number.Width / 2.0, 958.0);
        }
        assert_eq!(markers[1].number.Y, 337.0);
        assert_eq!(markers[4].number.Y, 641.0, "unknown fourth row must not shift later rows");
        let moved = gold_markers(RectF { X: 368.0, Y: 148.0, ..frame }, gold);
        assert_eq!(moved[0].number.X, markers[1].number.X - 12.0);
        assert_eq!(moved[0].number.Y, markers[1].number.Y - 87.0);
        let scaled = gold_markers(RectF { X: 0.0, Y: 0.0, Width: 2312.0, Height: 930.0 }, gold);
        assert_eq!(scaled[0].bar.as_ref().unwrap().Width, 6.0);
        let mut data = sample();
        data.scoreboard = Some(super::super::super::render::Bounds { x: 380, y: 235, width: 1156, height: 465 });
        assert!(widgets(1920, 1080, &data).iter().any(|w| matches!(w, Widget::Gold(..))));
    }

    #[test]
    fn gold_numbers_and_rings_read_like_dpm() {
        assert_eq!(short(19_640, false), "19.6K");
        assert_eq!(short(2_100, true), "+2.1K");
        assert_eq!(short(-700, true), "-700");
        assert_eq!(short(0, true), "+0");
        let ring = |seconds, period| {
            elapsed(Objective {
                label: "Dragon",
                seconds,
                period,
            })
        };
        assert_eq!(ring(300, 300), 0.0);
        assert_eq!(ring(150, 300), 0.5);
        assert_eq!(ring(0, 300), 1.0);
        assert_eq!(ring(10, 0), 1.0);
    }
}
