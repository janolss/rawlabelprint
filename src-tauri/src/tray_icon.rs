//! Compose tray icons for idle / attention / error across platforms.
//!
//! macOS uses monochrome template silhouettes (OS tints the menu bar glyph).
//! Linux and Windows draw a coloured badge on top of the base icon.

use tauri::image::Image;

const TEMPLATE_PNG: &[u8] = include_bytes!("../icons/tray.png");
const WINDOWS_PNG: &[u8] = include_bytes!("../icons/32x32.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayKind {
    Idle,
    Attention,
    Error,
}

/// Per-OS tray rendering. Variants unused on the current target are still
/// exercised by unit tests that cover all platforms in one binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlatformStyle {
    /// Black+alpha template; badge is part of the silhouette.
    MacTemplate,
    /// Template glyph (white on dark panels) plus a coloured badge.
    Linux { on_dark: bool },
    /// Full-colour app icon plus a coloured badge.
    WindowsColor,
}

/// Build the tray image for the current kind and platform style.
pub fn build(kind: TrayKind, style: PlatformStyle) -> tauri::Result<Image<'static>> {
    let (mut rgba, width, height) = base_rgba(style)?;
    match style {
        PlatformStyle::Linux { on_dark: true } => paint_template_white(&mut rgba),
        PlatformStyle::MacTemplate
        | PlatformStyle::Linux { on_dark: false }
        | PlatformStyle::WindowsColor => {}
    }
    match (kind, style) {
        (TrayKind::Idle, _) => {}
        (TrayKind::Attention, PlatformStyle::MacTemplate) => {
            draw_attention_silhouette(&mut rgba, width, height);
        }
        (TrayKind::Error, PlatformStyle::MacTemplate) => {
            draw_error_silhouette(&mut rgba, width, height);
        }
        (TrayKind::Attention, _) => {
            draw_attention_badge(&mut rgba, width, height);
        }
        (TrayKind::Error, _) => {
            draw_error_badge(&mut rgba, width, height);
        }
    }
    Ok(Image::new_owned(rgba, width, height))
}

fn base_rgba(style: PlatformStyle) -> tauri::Result<(Vec<u8>, u32, u32)> {
    let bytes = match style {
        PlatformStyle::WindowsColor => WINDOWS_PNG,
        PlatformStyle::MacTemplate | PlatformStyle::Linux { .. } => TEMPLATE_PNG,
    };
    let image = Image::from_bytes(bytes)?;
    Ok((image.rgba().to_vec(), image.width(), image.height()))
}

pub fn paint_template_white(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        if px[3] != 0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
    }
}

fn badge_geometry(width: u32, height: u32) -> (i32, i32, i32) {
    let r = (width.min(height) as i32 / 5).max(3);
    let cx = width as i32 - r - 1;
    let cy = height as i32 - r - 1;
    (cx, cy, r)
}

fn draw_attention_silhouette(rgba: &mut [u8], width: u32, height: u32) {
    let (cx, cy, r) = badge_geometry(width, height);
    // Template-safe: opaque black disk the OS will tint with the menu bar.
    fill_circle(rgba, width, height, cx, cy, r, [0, 0, 0, 255]);
}

fn draw_error_silhouette(rgba: &mut [u8], width: u32, height: u32) {
    let (cx, cy, r) = badge_geometry(width, height);
    fill_circle(rgba, width, height, cx, cy, r, [0, 0, 0, 255]);
    // Punch a "!" so the tinted silhouette reads as an alert badge.
    let bar_w = (r / 4).max(1);
    let bar_top = cy - r / 2;
    let bar_bottom = cy + r / 6;
    clear_rect(
        rgba,
        width,
        height,
        [cx - bar_w, bar_top, cx + bar_w, bar_bottom],
    );
    clear_rect(
        rgba,
        width,
        height,
        [cx - bar_w, cy + r / 3, cx + bar_w, cy + r / 2 + bar_w],
    );
}

fn draw_attention_badge(rgba: &mut [u8], width: u32, height: u32) {
    let (cx, cy, r) = badge_geometry(width, height);
    // Soft outline for contrast on both light and dark glyphs.
    fill_circle(rgba, width, height, cx, cy, r + 1, [255, 255, 255, 230]);
    fill_circle(rgba, width, height, cx, cy, r, [245, 158, 11, 255]); // amber
}

fn draw_error_badge(rgba: &mut [u8], width: u32, height: u32) {
    let (cx, cy, r) = badge_geometry(width, height);
    fill_circle(rgba, width, height, cx, cy, r + 1, [255, 255, 255, 230]);
    fill_circle(rgba, width, height, cx, cy, r, [220, 38, 38, 255]); // red
    let bar_w = (r / 5).max(1);
    let bar_top = cy - r / 2;
    let bar_bottom = cy + r / 8;
    fill_rect(
        rgba,
        width,
        height,
        [cx - bar_w, bar_top, cx + bar_w, bar_bottom],
        [255, 255, 255, 255],
    );
    fill_circle(
        rgba,
        width,
        height,
        cx,
        cy + r / 2,
        bar_w + 1,
        [255, 255, 255, 255],
    );
}

fn fill_circle(rgba: &mut [u8], width: u32, height: u32, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    if r < 0 {
        return;
    }
    let r2 = r * r;
    for y in (cy - r)..=(cy + r) {
        for x in (cx - r)..=(cx + r) {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                set_pixel(rgba, width, x as u32, y as u32, color);
            }
        }
    }
}

fn fill_rect(rgba: &mut [u8], width: u32, height: u32, rect: [i32; 4], color: [u8; 4]) {
    let [x0, y0, x1, y1] = rect;
    for y in y0.min(y1)..=y0.max(y1) {
        for x in x0.min(x1)..=x0.max(x1) {
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            set_pixel(rgba, width, x as u32, y as u32, color);
        }
    }
}

fn clear_rect(rgba: &mut [u8], width: u32, height: u32, rect: [i32; 4]) {
    fill_rect(rgba, width, height, rect, [0, 0, 0, 0]);
}

fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 4]) {
    let i = ((y * width + x) * 4) as usize;
    if i + 3 < rgba.len() {
        rgba[i..i + 4].copy_from_slice(&color);
    }
}

#[cfg(test)]
fn pixel_at(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// True when RGB channels differ enough from greyscale (coloured badge present).
#[cfg(test)]
fn has_chromatic_pixel(rgba: &[u8]) -> bool {
    for px in rgba.as_chunks::<4>().0 {
        if px[3] < 200 {
            continue;
        }
        let max = px[0].max(px[1]).max(px[2]);
        let min = px[0].min(px[1]).min(px[2]);
        if max.saturating_sub(min) > 40 {
            return true;
        }
    }
    false
}

/// Glyph pixels (alpha > 0) share the same RGB — required for macOS templates.
#[cfg(test)]
fn glyph_is_monochrome(rgba: &[u8]) -> bool {
    let mut first: Option<[u8; 3]> = None;
    for px in rgba.as_chunks::<4>().0 {
        if px[3] == 0 {
            continue;
        }
        let rgb = [px[0], px[1], px[2]];
        match first {
            None => first = Some(rgb),
            Some(base) if base != rgb => return false,
            _ => {}
        }
    }
    first.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_linux_light_has_no_chromatic_badge() {
        let image = build(TrayKind::Idle, PlatformStyle::Linux { on_dark: false }).unwrap();
        assert!(!has_chromatic_pixel(image.rgba()));
    }

    #[test]
    fn attention_and_error_linux_are_chromatic() {
        let attention =
            build(TrayKind::Attention, PlatformStyle::Linux { on_dark: false }).unwrap();
        let error = build(TrayKind::Error, PlatformStyle::Linux { on_dark: true }).unwrap();
        assert!(has_chromatic_pixel(attention.rgba()));
        assert!(has_chromatic_pixel(error.rgba()));
        assert_ne!(attention.rgba(), error.rgba());
    }

    #[test]
    fn windows_attention_differs_from_idle() {
        let idle = build(TrayKind::Idle, PlatformStyle::WindowsColor).unwrap();
        let attention = build(TrayKind::Attention, PlatformStyle::WindowsColor).unwrap();
        assert_ne!(idle.rgba(), attention.rgba());
        assert!(has_chromatic_pixel(attention.rgba()));
    }

    #[test]
    fn mac_variants_stay_monochrome_templates() {
        for kind in [TrayKind::Idle, TrayKind::Attention, TrayKind::Error] {
            let image = build(kind, PlatformStyle::MacTemplate).unwrap();
            assert!(
                glyph_is_monochrome(image.rgba()),
                "{kind:?} must remain a monochrome template"
            );
        }
        let idle = build(TrayKind::Idle, PlatformStyle::MacTemplate).unwrap();
        let attention = build(TrayKind::Attention, PlatformStyle::MacTemplate).unwrap();
        let error = build(TrayKind::Error, PlatformStyle::MacTemplate).unwrap();
        assert_ne!(idle.rgba(), attention.rgba());
        assert_ne!(attention.rgba(), error.rgba());
    }

    #[test]
    fn white_template_keeps_alpha_and_preserves_badge_color() {
        let image = Image::from_bytes(TEMPLATE_PNG).unwrap();
        let mut rgba = image.rgba().to_vec();
        paint_template_white(&mut rgba);
        for px in rgba.as_chunks::<4>().0 {
            if px[3] != 0 {
                assert_eq!(&px[..3], &[255, 255, 255]);
            }
        }
        draw_attention_badge(&mut rgba, image.width(), image.height());
        assert!(has_chromatic_pixel(&rgba));
        let (cx, cy, _) = badge_geometry(image.width(), image.height());
        let badge = pixel_at(&rgba, image.width(), cx as u32, cy as u32);
        assert_eq!(&badge[..3], &[245, 158, 11]);
    }
}
