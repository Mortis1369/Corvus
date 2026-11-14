//! Icon helpers: the embedded logo PNG, and extracting a tool's own icon out of its exe.
//!
//! Exe icon extraction is plain Win32: `PrivateExtractIconsW` gives an `HICON` at the size we
//! ask for, `GetIconInfo` hands us its colour and mask bitmaps, `GetDIBits` copies them out as
//! 32-bit top-down pixels. Everything is read-only and every handle is released before return.
//! Any failure returns `None` and the tile falls back to a letter glyph.

use egui::ColorImage;
use std::path::Path;

/// Decode an embedded PNG into an egui image.
pub fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (w, h) = img.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        img.as_raw(),
    ))
}

/// Same PNG as the window/taskbar icon.
pub fn icon_data_from_png(bytes: &[u8]) -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// Pull the first icon out of an exe (or dll/ico) at roughly `size` px.
pub fn extract_icon(path: &Path, size: u32) -> Option<ColorImage> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, PrivateExtractIconsW, HICON};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut hicon: HICON = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `hicon` is a valid out-slot for
    // exactly the one icon we request (nicons = 1).
    let n = unsafe {
        PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            size as i32,
            size as i32,
            &mut hicon,
            std::ptr::null_mut(),
            1,
            0,
        )
    };
    if n == 0 || n == u32::MAX || hicon.is_null() {
        return None;
    }
    // SAFETY: hicon is a valid icon handle we own; it is destroyed exactly once below.
    let img = unsafe { hicon_to_image(hicon) };
    unsafe { DestroyIcon(hicon) };
    img
}

/// Convert an HICON to RGBA. Caller still owns (and must destroy) the icon.
unsafe fn hicon_to_image(
    hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> Option<ColorImage> {
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut info) == 0 {
        return None;
    }
    // From here on, both bitmaps must be deleted on every path.
    let result = (|| {
        if info.hbmColor.is_null() {
            return None; // monochrome icon; not worth the special case
        }
        let mut bm: BITMAP = std::mem::zeroed();
        if GetObjectW(
            info.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bm as *mut BITMAP as *mut _,
        ) == 0
        {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 || w > 1024 || h > 1024 {
            return None;
        }
        let hdc = GetDC(std::ptr::null_mut());
        if hdc.is_null() {
            return None;
        }
        let color = read_bits(hdc, info.hbmColor, w, h);
        let mask = read_bits(hdc, info.hbmMask, w, h);
        ReleaseDC(std::ptr::null_mut(), hdc);
        let mut color = color?;

        // Icons without an alpha channel come back with alpha = 0 everywhere; use the AND mask.
        let (pixels, _) = color.as_chunks_mut::<4>();
        if pixels.iter().all(|p| p[3] == 0) {
            let mask = mask?;
            let (mask_px, _) = mask.as_chunks::<4>();
            for (px, m) in pixels.iter_mut().zip(mask_px) {
                px[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        // BGRA → RGBA
        for px in pixels.iter_mut() {
            px.swap(0, 2);
        }
        Some(ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            &color,
        ))
    })();

    unsafe fn read_bits(
        hdc: windows_sys::Win32::Graphics::Gdi::HDC,
        hbm: HBITMAP,
        w: i32,
        h: i32,
    ) -> Option<Vec<u8>> {
        if hbm.is_null() {
            return None;
        }
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // negative = top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            hdc,
            hbm,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        if lines == 0 {
            None
        } else {
            Some(buf)
        }
    }

    DeleteObject(info.hbmColor);
    DeleteObject(info.hbmMask);
    result
}
