use super::super::model::{DisplayData, Gold, Objective};
use super::{icons, Surface, PULSE_STEPS};
use windows_sys::Win32::Graphics::GdiPlus::*;

// ARGB palette, matched to Swapper's dark theme and DPM's widget colours.
const PANEL: u32 = 0xE6101522;
const EDGE: u32 = 0x33FFFFFF;
const TRACK: u32 = 0x40FFFFFF;
const TEXT: u32 = 0xFFE9ECF4;
const MUTED: u32 = 0xFF8B93A7;
const BLUE: u32 = 0xFF6E9BFF;
const RED: u32 = 0xFFFF6B7A;
const GOLD: u32 = 0xFFE3B45C;
/// Swapper's signature green (`--brand-green`) on graphite, for the level-up hint.
const BRAND_GREEN: u32 = 0xFFC5E39E;
const HINT_FILL: u32 = 0xF2181A17;

const GOLD_BOX: f32 = 44.0;
const LANE_BADGE: f32 = 26.0;

impl Surface {
    /// `19.6K  ● +2.1K  17.5K` in a gold frame, lane difference badge below.
    pub(super) fn gold(&self, rect: RectF, gold: Gold) -> Result<(), String> {
        let s = self.scale;
        let main = RectF {
            Height: GOLD_BOX * s,
            ..rect
        };
        self.rounded(main, 8.0 * s, PANEL, Some((GOLD & 0xCCFFFFFF, 1.5 * s)))?;
        let column = main.Width / 3.0;
        let at = |i: f32| RectF {
            X: main.X + column * i,
            Width: column,
            ..main
        };
        let diff = gold.ours - gold.theirs;
        self.text(at(0.0), &short(gold.ours, false), self.large, BLUE)?;
        self.text(at(2.0), &short(gold.theirs, false), self.large, RED)?;
        let coin = 12.0 * s;
        let middle = at(1.0);
        self.circle(
            middle.X + 4.0 * s,
            middle.Y + (middle.Height - coin) / 2.0,
            coin,
            GOLD,
        )?;
        self.text(
            RectF {
                X: middle.X + coin,
                Width: middle.Width - coin,
                ..middle
            },
            &short(diff, true),
            self.large,
            sign_color(diff),
        )?;
        let Some(lane) = gold.lane else {
            return std::result::Result::Ok(());
        };
        let badge = RectF {
            X: rect.X + (rect.Width - 132.0 * s) / 2.0,
            Y: main.Y + main.Height + 6.0 * s,
            Width: 132.0 * s,
            Height: LANE_BADGE * s,
        };
        self.rounded(badge, badge.Height / 2.0, PANEL, Some((EDGE, 1.0 * s)))?;
        let label = RectF {
            X: badge.X + 8.0 * s,
            Width: 52.0 * s,
            ..badge
        };
        self.text(label, "LANE", self.small, MUTED)?;
        self.text(
            RectF {
                X: label.X + label.Width,
                Width: badge.Width - label.Width - 16.0 * s,
                ..badge
            },
            &short(lane, true),
            self.medium,
            sign_color(lane),
        )
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

fn sign_color(value: i64) -> u32 {
    if value >= 0 {
        BLUE
    } else {
        RED
    }
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
    if let Some(gold) = data.gold {
        let gw = (360.0 * s).min(w);
        widgets.push(Widget::Gold(
            RectF {
                X: (w - gw) / 2.0,
                Y: h * 0.155,
                Width: gw,
                Height: (GOLD_BOX + 6.0 + LANE_BADGE) * s,
            },
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
                ours: 19_600,
                theirs: 17_500,
                lane: Some(-700),
            }),
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
            assert_eq!(widgets.len(), 4);
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
