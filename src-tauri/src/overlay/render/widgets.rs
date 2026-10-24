use super::super::model::{DisplayData, Gold, Objective, ROLES};
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

const GOLD_WIDTH: f32 = 280.0;
const GOLD_HEAD: f32 = 30.0;
const GOLD_ROW: f32 = 20.0;
const GOLD_PAD: f32 = 8.0;
/// A role gap this large fills its half of the row; bigger gaps cap there.
const ROLE_GAP_FULL: f64 = 3000.0;
const ROLE_LABELS: [&str; 5] = ["TOP", "JNG", "MID", "BOT", "SUP"];
const ROW_HIGHLIGHT: u32 = 0x1FFFFFFF;

fn gold_height() -> f32 {
    GOLD_PAD * 2.0 + GOLD_HEAD + ROLES.len() as f32 * GOLD_ROW
}

/// The bar length (share of its half) for a role gap.
fn role_share(gap: i64) -> f32 {
    (gap.unsigned_abs() as f64 / ROLE_GAP_FULL).min(1.0) as f32
}

impl Surface {
    /// Team totals (`19.6K ● +2.1K 17.5K`) over one row per role: from a centre
    /// line a bar grows left in blue when blue side's player leads that role, or
    /// right in red when red's does, with the gap at its end. Your role is lit.
    pub(super) fn gold(&self, rect: RectF, gold: Gold) -> Result<(), String> {
        let s = self.scale;
        self.rounded(rect, 8.0 * s, PANEL, Some((GOLD & 0xCCFFFFFF, 1.5 * s)))?;
        let inner = RectF {
            X: rect.X + GOLD_PAD * s,
            Y: rect.Y + GOLD_PAD * s,
            Width: rect.Width - 2.0 * GOLD_PAD * s,
            Height: rect.Height - 2.0 * GOLD_PAD * s,
        };
        let column = inner.Width / 3.0;
        let head = |i: f32| RectF {
            X: inner.X + column * i,
            Y: inner.Y,
            Width: column,
            Height: GOLD_HEAD * s,
        };
        let diff = gold.blue - gold.red;
        self.text(head(0.0), &short(gold.blue, false), self.large, BLUE)?;
        self.text(head(2.0), &short(gold.red, false), self.large, RED)?;
        let coin = 11.0 * s;
        let middle = head(1.0);
        self.circle(middle.X + 4.0 * s, middle.Y + (middle.Height - coin) / 2.0, coin, GOLD)?;
        self.text(
            RectF { X: middle.X + coin, Width: middle.Width - coin, ..middle },
            &short(diff.abs(), false),
            self.medium,
            if diff == 0 { MUTED } else { sign_color(diff) },
        )?;
        let label_width = 34.0 * s;
        let number = 42.0 * s;
        let bars_x = inner.X + label_width;
        let half = (inner.Width - label_width) / 2.0;
        let centre = bars_x + half;
        for (i, gap) in gold.roles.iter().enumerate() {
            let row = RectF {
                X: inner.X,
                Y: inner.Y + GOLD_HEAD * s + i as f32 * GOLD_ROW * s,
                Width: inner.Width,
                Height: GOLD_ROW * s,
            };
            let mine = gold.mine == Some(i);
            if mine {
                self.rounded(row, 4.0 * s, ROW_HIGHLIGHT, None)?;
            }
            self.text(RectF { Width: label_width, ..row }, ROLE_LABELS[i], self.small, if mine { TEXT } else { MUTED })?;
            let mid_y = row.Y + row.Height / 2.0;
            self.rounded(RectF { X: bars_x, Y: mid_y - 1.0 * s, Width: half * 2.0, Height: 2.0 * s }, 1.0 * s, TRACK & 0x30FFFFFF, None)?;
            self.rounded(RectF { X: centre - 1.0 * s, Y: row.Y + 3.0 * s, Width: 2.0 * s, Height: row.Height - 6.0 * s }, 1.0 * s, TEXT & 0x99FFFFFF, None)?;
            // An unknown role (missing or shared position) keeps only the centre tick.
            let Some(gap) = *gap else { continue };
            let length = role_share(gap) * (half - number);
            let (color, bar, label) = if gap >= 0 {
                (BLUE, RectF { X: centre - length, Y: mid_y - 3.0 * s, Width: length, Height: 6.0 * s }, centre - length - number)
            } else {
                (RED, RectF { X: centre, Y: mid_y - 3.0 * s, Width: length, Height: 6.0 * s }, centre + length)
            };
            if length > 0.5 {
                self.rounded(bar, 3.0 * s, color, None)?;
            }
            self.text(RectF { X: label, Width: number, ..row }, &short(gap.abs(), false), self.small, if gap == 0 { MUTED } else { color })?;
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
        let gw = (GOLD_WIDTH * s).min(w);
        widgets.push(Widget::Gold(
            RectF {
                X: (w - gw) / 2.0,
                // High enough to clear the Tab scoreboard it is shown with.
                Y: h * 0.07,
                Width: gw,
                Height: (gold_height() * s).min(h * 0.9),
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
                blue: 19_600,
                red: 17_500,
                roles: [Some(1_600), Some(-4_200), Some(2_400), None, Some(0)],
                mine: Some(2),
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
    fn role_bars_scale_with_the_gap_and_cap_at_full() {
        assert_eq!(role_share(0), 0.0);
        assert_eq!(role_share(1_500), 0.5);
        assert_eq!(role_share(-1_500), 0.5, "red leads draw just as long");
        assert_eq!(role_share(9_000), 1.0);
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
