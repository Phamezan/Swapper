//! One layered HWND, no WebView and no frame loop. GDI+ draws infrequent text
//! updates directly into a premultiplied-alpha DIB (no GPU device/swap chain).
mod widgets;

use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::GdiPlus::{self as gp, *};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::icons;
use super::model::DisplayData;
use widgets::{scale, widgets, Widget};

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

/// Steps per diamond animation cycle, at the overlay's 50 ms tick.
pub const PULSE_STEPS: u8 = 20;

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
                Widget::Arrow(rect, pulse) => self.arrow(rect, pulse)?,
            }
        }
        // Flush before GDI consumes the same DIB memory.
        unsafe { status(GdipFlush(self.graphics, FlushIntentionSync)) }
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
    fn icon(
        &self,
        rect: RectF,
        mask: &[u8; icons::SIZE * icons::SIZE],
        color: u32,
    ) -> Result<(), String> {
        let rgb = color & 0x00FF_FFFF;
        let pixels: Vec<u32> = mask
            .iter()
            .map(|alpha| (u32::from(*alpha) << 24) | rgb)
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

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
            pulse: 0,
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
            // Every role filled, so each bar state shows (blue lead, red lead, even, unknown).
            gold: super::super::model::gold(&fixture).map(|gold| super::super::model::Gold {
                roles: [Some(1_600), Some(-4_200), gold.roles[2], None, Some(0)],
                ..gold
            }),
            objectives: super::super::model::objectives(&fixture),
            skill: Some('R'),
            arrow: Some([600, 600, 20, 8]),
            pulse: 0,
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
