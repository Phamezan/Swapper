//! One layered HWND, no WebView and no frame loop. GDI+ draws infrequent text
//! updates directly into a premultiplied-alpha DIB (no GPU device/swap chain).
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::GdiPlus::{self as gp, *};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::icons;
use super::model::{DisplayData, Gold, Objective};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn game_bounds(pids: &[u32]) -> Option<Bounds> {
    unsafe {
        let title = wide("League of Legends (TM) Client");
        let game = FindWindowW(null(), title.as_ptr());
        if game.is_null()
            || IsIconic(game) != 0
            || IsWindowVisible(game) == 0
            || GetForegroundWindow() != game
        {
            return None;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(game, &mut pid);
        if !pids.contains(&pid) {
            return None;
        }
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        if GetClientRect(game, &mut rect) == 0 || ClientToScreen(game, &mut origin) == 0 {
            return None;
        }
        (rect.right >= 360 && rect.bottom >= 240).then_some(Bounds {
            x: origin.x,
            y: origin.y,
            width: rect.right,
            height: rect.bottom,
        })
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

fn status(code: Status) -> Result<(), String> {
    if code == gp::Ok {
        std::result::Result::Ok(())
    } else {
        Err(format!("Overlay GDI+ error {code}"))
    }
}

pub struct Renderer {
    hwnd: HWND,
    token: usize,
    surface: Option<Surface>,
    last: Option<(Bounds, DisplayData)>,
    visible: bool,
}

impl Renderer {
    pub fn new() -> Result<Self, String> {
        let mut renderer = Self {
            hwnd: null_mut(),
            token: 0,
            surface: None,
            last: None,
            visible: false,
        };
        unsafe {
            let input = GdiplusStartupInput {
                GdiplusVersion: 1,
                ..Default::default()
            };
            status(GdiplusStartup(&mut renderer.token, &input, null_mut()))?;
            let class = wide("SwapperInGameOverlay");
            let instance = GetModuleHandleW(null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != 1410 {
                return Err("Could not register overlay window".into());
            }
            renderer.hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            if renderer.hwnd.is_null() {
                return Err("Could not create overlay window".into());
            }
            // Keep the overlay out of screen captures, so the arrow finder never sees its own drawing.
            SetWindowDisplayAffinity(renderer.hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        std::result::Result::Ok(renderer)
    }

    pub fn pump(&self) {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    pub fn show(&mut self, bounds: Bounds, data: &DisplayData) -> Result<(), String> {
        if self.last.as_ref().is_none_or(|(old_bounds, old)| {
            old_bounds.width != bounds.width || old_bounds.height != bounds.height || old != data
        }) {
            if self
                .surface
                .as_ref()
                .is_none_or(|s| s.width != bounds.width || s.height != bounds.height)
            {
                self.surface = Some(Surface::new(bounds.width, bounds.height)?);
            }
            let surface = self.surface.as_mut().expect("surface created");
            surface.draw(data)?;
            unsafe {
                let point = POINT {
                    x: bounds.x,
                    y: bounds.y,
                };
                let size = SIZE {
                    cx: bounds.width,
                    cy: bounds.height,
                };
                let source = POINT::default();
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                if UpdateLayeredWindow(
                    self.hwnd,
                    null_mut(),
                    &point,
                    &size,
                    surface.dc,
                    &source,
                    0,
                    &blend,
                    ULW_ALPHA,
                ) == 0
                {
                    return Err("Could not update overlay window".into());
                }
            }
            self.last = Some((bounds, data.clone()));
        } else if let Some((previous, _)) = self.last.as_mut() {
            if previous.x != bounds.x || previous.y != bounds.y {
                // A move needs only a compositor position update, no bitmap redraw.
                unsafe {
                    SetWindowPos(
                        self.hwnd,
                        HWND_TOPMOST,
                        bounds.x,
                        bounds.y,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOSIZE,
                    );
                }
                *previous = bounds;
            }
        }
        if !self.visible {
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
            self.visible = true;
        }
        std::result::Result::Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.surface.take();
        unsafe {
            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
            if self.token != 0 {
                GdiplusShutdown(self.token);
            }
        }
    }
}

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    image: *mut GpBitmap,
    graphics: *mut GpGraphics,
    family: *mut GpFontFamily,
    large: *mut GpFont,
    medium: *mut GpFont,
    small: *mut GpFont,
    format: *mut GpStringFormat,
    scale: f32,
    width: i32,
    height: i32,
}

// ARGB palette, matched to Swapper's dark theme and DPM's widget colours.
const PANEL: u32 = 0xE6101522;
const EDGE: u32 = 0x33FFFFFF;
const TRACK: u32 = 0x40FFFFFF;
const TEXT: u32 = 0xFFE9ECF4;
const MUTED: u32 = 0xFF8B93A7;
const BLUE: u32 = 0xFF6E9BFF;
const RED: u32 = 0xFFFF6B7A;
const GOLD: u32 = 0xFFE3B45C;
const ACCENT: u32 = 0xFF7C83FD;

const GOLD_BOX: f32 = 44.0;
const LANE_BADGE: f32 = 26.0;

impl Surface {
    fn new(width: i32, height: i32) -> Result<Self, String> {
        let mut surface = Self {
            dc: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
            image: null_mut(),
            graphics: null_mut(),
            family: null_mut(),
            large: null_mut(),
            medium: null_mut(),
            small: null_mut(),
            format: null_mut(),
            scale: scale(height),
            width,
            height,
        };
        unsafe {
            surface.dc = CreateCompatibleDC(null_mut());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            surface.bitmap =
                CreateDIBSection(surface.dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if surface.dc.is_null() || surface.bitmap.is_null() || bits.is_null() {
                return Err("Could not allocate overlay bitmap".into());
            }
            surface.previous = SelectObject(surface.dc, surface.bitmap);
            // PixelFormat32bppPARGB: native BGRA, premultiplied for UpdateLayeredWindow.
            status(GdipCreateBitmapFromScan0(
                width,
                height,
                width * 4,
                0x000E200B,
                bits.cast(),
                &mut surface.image,
            ))?;
            status(GdipGetImageGraphicsContext(
                surface.image.cast(),
                &mut surface.graphics,
            ))?;
            status(GdipSetTextRenderingHint(
                surface.graphics,
                TextRenderingHintAntiAliasGridFit,
            ))?;
            status(GdipSetSmoothingMode(
                surface.graphics,
                SmoothingModeAntiAlias,
            ))?;
            status(GdipCreateFontFamilyFromName(
                wide("Segoe UI").as_ptr(),
                null_mut(),
                &mut surface.family,
            ))?;
            let s = surface.scale;
            for (font, size) in [
                (&mut surface.large, 21.0),
                (&mut surface.medium, 16.0),
                (&mut surface.small, 12.0),
            ] {
                status(GdipCreateFont(
                    surface.family,
                    size * s,
                    FontStyleBold,
                    UnitPixel,
                    font,
                ))?;
            }
            status(GdipCreateStringFormat(0, 0, &mut surface.format))?;
            status(GdipSetStringFormatAlign(
                surface.format,
                StringAlignmentCenter,
            ))?;
            status(GdipSetStringFormatLineAlign(
                surface.format,
                StringAlignmentCenter,
            ))?;
        }
        std::result::Result::Ok(surface)
    }

    fn draw(&mut self, data: &DisplayData) -> Result<(), String> {
        unsafe {
            status(GdipGraphicsClear(self.graphics, 0))?;
        }
        for widget in widgets(self.width, self.height, data) {
            match widget {
                Widget::Gold(rect, gold) => self.gold(rect, gold)?,
                Widget::Objective(rect, objective) => self.objective(rect, objective)?,
                Widget::Arrow(rect) => self.arrow(rect)?,
            }
        }
        // Flush before GDI consumes the same DIB memory.
        unsafe { status(GdipFlush(self.graphics, FlushIntentionSync)) }
    }

    /// `19.6K  ● +2.1K  17.5K` in a gold frame, lane difference badge below.
    fn gold(&self, rect: RectF, gold: Gold) -> Result<(), String> {
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
    fn objective(&self, rect: RectF, objective: Objective) -> Result<(), String> {
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

    /// Frames the game's own level-up button: a crisp rounded border with a soft
    /// outer glow and a faint tint, so the game's arrow stays visible inside it,
    /// plus three stacked diamonds above, fading upward. Sized from the arrow
    /// itself, so it follows the HUD scale.
    fn arrow(&self, rect: RectF) -> Result<(), String> {
        let unit = rect.Width.max(rect.Height * 2.0);
        let side = unit * 1.5;
        let (cx, cy) = (rect.X + rect.Width / 2.0, rect.Y + rect.Height / 2.0);
        let frame = RectF { X: cx - side / 2.0, Y: cy - side / 2.0, Width: side, Height: side };
        let radius = unit * 0.2;
        let glow = |grow: f32| RectF {
            X: frame.X - grow,
            Y: frame.Y - grow,
            Width: frame.Width + 2.0 * grow,
            Height: frame.Height + 2.0 * grow,
        };
        let tint = |alpha: u32| (ACCENT & 0x00FF_FFFF) | (alpha << 24);
        for (grow, alpha) in [(unit * 0.22, 0x18), (unit * 0.12, 0x30)] {
            self.rounded(glow(grow), radius + grow, 0, Some((tint(alpha), unit * 0.12)))?;
        }
        self.rounded(frame, radius, tint(0x1C), Some((ACCENT, (unit * 0.09).max(1.5))))?;
        let mut y = frame.Y - unit * 0.32;
        for (size, alpha) in [(0.26, 0xFF), (0.2, 0xB0), (0.15, 0x66)] {
            let half = unit * size / 2.0;
            self.diamond(cx, y - half, half, tint(alpha))?;
            y -= half * 2.0 + unit * 0.14;
        }
        std::result::Result::Ok(())
    }

    fn diamond(&self, x: f32, y: f32, half: f32, color: u32) -> Result<(), String> {
        let points = [
            PointF { X: x, Y: y - half },
            PointF { X: x + half, Y: y },
            PointF { X: x, Y: y + half },
            PointF { X: x - half, Y: y },
        ];
        self.with_brush(color, |brush| unsafe {
            GdipFillPolygon(self.graphics, brush, points.as_ptr(), 4, FillModeAlternate)
        })
    }

    fn text(&self, rect: RectF, text: &str, font: *mut GpFont, color: u32) -> Result<(), String> {
        let text = wide(text);
        self.with_brush(color, |brush| unsafe {
            GdipDrawString(
                self.graphics,
                text.as_ptr(),
                (text.len() - 1) as i32,
                font,
                &rect,
                self.format,
                brush,
            )
        })
    }

    fn rounded(
        &self,
        r: RectF,
        radius: f32,
        fill: u32,
        edge: Option<(u32, f32)>,
    ) -> Result<(), String> {
        let d = (radius * 2.0).min(r.Width).min(r.Height);
        unsafe {
            let mut path = null_mut();
            status(GdipCreatePath(FillModeAlternate, &mut path))?;
            GdipAddPathArc(path, r.X, r.Y, d, d, 180.0, 90.0);
            GdipAddPathArc(path, r.X + r.Width - d, r.Y, d, d, 270.0, 90.0);
            GdipAddPathArc(path, r.X + r.Width - d, r.Y + r.Height - d, d, d, 0.0, 90.0);
            GdipAddPathArc(path, r.X, r.Y + r.Height - d, d, d, 90.0, 90.0);
            GdipClosePathFigure(path);
            let mut result =
                self.with_brush(fill, |brush| GdipFillPath(self.graphics, brush, path));
            if let (true, Some((color, width))) = (result.is_ok(), edge) {
                result = self.with_pen(color, width, |pen| GdipDrawPath(self.graphics, pen, path));
            }
            GdipDeletePath(path);
            result
        }
    }

    fn arc(&self, r: RectF, sweep: f32, color: u32, width: f32) -> Result<(), String> {
        if sweep <= 0.0 {
            return std::result::Result::Ok(());
        }
        self.with_pen(color, width, |pen| unsafe {
            GdipSetPenStartCap(pen, LineCapRound);
            GdipSetPenEndCap(pen, LineCapRound);
            GdipDrawArc(
                self.graphics,
                pen,
                r.X,
                r.Y,
                r.Width,
                r.Height,
                -90.0,
                sweep,
            )
        })
    }

    /// Draws an alpha mask from `icons` tinted with `color`.
    fn icon(&self, rect: RectF, mask: &[u8; icons::SIZE * icons::SIZE], color: u32) -> Result<(), String> {
        let rgb = color & 0x00FF_FFFF;
        let pixels: Vec<u32> = mask.iter().map(|alpha| (u32::from(*alpha) << 24) | rgb).collect();
        unsafe {
            let mut image = null_mut();
            // PixelFormat32bppARGB (straight alpha); GDI+ premultiplies when drawing.
            status(GdipCreateBitmapFromScan0(
                icons::SIZE as i32,
                icons::SIZE as i32,
                icons::SIZE as i32 * 4,
                0x0026_200A,
                pixels.as_ptr().cast_mut().cast(),
                &mut image,
            ))?;
            GdipSetInterpolationMode(self.graphics, InterpolationModeHighQualityBicubic);
            let result = status(GdipDrawImageRect(
                self.graphics,
                image.cast(),
                rect.X,
                rect.Y,
                rect.Width,
                rect.Height,
            ));
            GdipDisposeImage(image.cast());
            result
        }
    }

    fn circle(&self, x: f32, y: f32, size: f32, color: u32) -> Result<(), String> {
        self.with_brush(color, |brush| unsafe {
            GdipFillEllipse(self.graphics, brush, x, y, size, size)
        })
    }

    fn with_brush(
        &self,
        color: u32,
        draw: impl FnOnce(*mut GpBrush) -> Status,
    ) -> Result<(), String> {
        unsafe {
            let mut brush = null_mut();
            status(GdipCreateSolidFill(color, &mut brush))?;
            let result = status(draw(brush.cast()));
            GdipDeleteBrush(brush.cast());
            result
        }
    }

    fn with_pen(
        &self,
        color: u32,
        width: f32,
        draw: impl FnOnce(*mut GpPen) -> Status,
    ) -> Result<(), String> {
        unsafe {
            let mut pen = null_mut();
            status(GdipCreatePen1(color, width, UnitPixel, &mut pen))?;
            let result = status(draw(pen));
            GdipDeletePen(pen);
            result
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.format.is_null() {
                GdipDeleteStringFormat(self.format);
            }
            for font in [self.large, self.medium, self.small] {
                if !font.is_null() {
                    GdipDeleteFont(font);
                }
            }
            if !self.family.is_null() {
                GdipDeleteFontFamily(self.family);
            }
            if !self.graphics.is_null() {
                GdipDeleteGraphics(self.graphics);
            }
            if !self.image.is_null() {
                GdipDisposeImage(self.image.cast());
            }
            if !self.previous.is_null() {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}

fn scale(height: i32) -> f32 {
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

enum Widget {
    Gold(RectF, Gold),
    Objective(RectF, Objective),
    Arrow(RectF),
}

fn widgets(width: i32, height: i32, data: &DisplayData) -> Vec<Widget> {
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
        widgets.push(Widget::Arrow(RectF {
            X: x as f32,
            Y: y as f32,
            Width: aw as f32,
            Height: ah as f32,
        }));
    }
    widgets
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
        }
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
            assert_eq!(widgets.iter().filter(|w| matches!(w, Widget::Arrow(_))).count(), 1);
            for widget in widgets {
                let (Widget::Gold(rect, _)
                | Widget::Objective(rect, _)
                | Widget::Arrow(rect)) = widget;
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

    #[test]
    fn native_renderer_draws_fixture_and_retains_clickthrough_styles() {
        let mut renderer = Renderer::new().unwrap();
        let foreground = unsafe { GetForegroundWindow() };
        let data = serde_json::from_str(include_str!("fixtures/allgamedata.json")).unwrap();
        let display = DisplayData {
            gold: super::super::model::gold(&data),
            objectives: super::super::model::objectives(&data),
            skill: Some('R'),
            arrow: Some([600, 600, 20, 8]),
        };
        let bounds = Bounds {
            x: -10000,
            y: -10000,
            width: 1280,
            height: 720,
        };
        renderer.show(bounds, &display).unwrap();
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        let surface = renderer.surface.as_ref().unwrap().bitmap;
        renderer.show(bounds, &display).unwrap();
        assert_eq!(renderer.surface.as_ref().unwrap().bitmap, surface);
        renderer
            .show(Bounds { x: -9999, ..bounds }, &display)
            .unwrap();
        assert_eq!(renderer.surface.as_ref().unwrap().bitmap, surface);
        unsafe {
            let mut transparent = 1;
            status(GdipBitmapGetPixel(
                renderer.surface.as_ref().unwrap().image,
                0,
                0,
                &mut transparent,
            ))
            .unwrap();
            assert_eq!(
                transparent, 0,
                "outside widgets must stay fully transparent"
            );
        }
        unsafe {
            let style = GetWindowLongW(renderer.hwnd, GWL_EXSTYLE) as u32;
            for flag in [
                WS_EX_LAYERED,
                WS_EX_TRANSPARENT,
                WS_EX_TOPMOST,
                WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW,
            ] {
                assert_ne!(style & flag, 0);
            }
        }
        renderer.hide();
        assert!(!renderer.visible);
        assert_eq!(unsafe { IsWindowVisible(renderer.hwnd) }, 0);
        let hwnd = renderer.hwnd;
        drop(renderer);
        assert_eq!(unsafe { IsWindow(hwnd) }, 0);
    }

    #[test]
    #[ignore = "exports a fixture PNG and shows a click-through preview for ten seconds"]
    fn preview_fixture() {
        let mut renderer = Renderer::new().unwrap();
        let fixture = serde_json::from_str(include_str!("fixtures/allgamedata.json")).unwrap();
        let data = DisplayData {
            gold: super::super::model::gold(&fixture),
            objectives: super::super::model::objectives(&fixture),
            skill: Some('R'),
            arrow: Some([600, 600, 20, 8]),
        };
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
        renderer.show(bounds, &data).unwrap();
        let path = std::env::temp_dir().join("swapper-overlay-fixture.png");
        let encoder = windows_sys::core::GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);
        unsafe {
            status(GdipSaveImageToFile(
                renderer.surface.as_ref().unwrap().image.cast(),
                wide(&path.to_string_lossy()).as_ptr(),
                &encoder,
                null(),
            ))
            .unwrap();
        }
        println!("Fixture PNG: {}", path.display());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < until {
            renderer.pump();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
