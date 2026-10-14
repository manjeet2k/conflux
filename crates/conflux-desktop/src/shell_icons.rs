//! Windows Shell association icons, queried without opening downloaded files.
use serde::Serialize;

const ICON_SIZE: usize = 256;

#[derive(Clone, Serialize)]
pub struct ShellIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn normalize_extension(extension: Option<String>) -> Result<String, String> {
    let extension = extension.unwrap_or_default();
    let extension = extension
        .trim()
        .strip_prefix('.')
        .unwrap_or(extension.trim());
    if extension.len() > 32 || !extension.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("Icon lookup requires a file extension, not a path".into());
    }
    Ok(extension.to_ascii_lowercase())
}

#[tauri::command]
pub async fn get_shell_icon(extension: Option<String>, folder: bool) -> Result<ShellIcon, String> {
    let extension = normalize_extension(extension)?;
    tokio::task::spawn_blocking(move || load_icon(&extension, folder))
        .await
        .map_err(|_| "Windows icon worker failed".to_string())?
        .map_err(|error| format!("Windows icon lookup failed: {error}"))
}

/// Recover straight-alpha RGBA from the icon composited on black and white.
/// This handles both alpha icons and older icons whose transparency uses an AND mask.
fn rgba_from_backgrounds(black: &[u8], white: &[u8]) -> Vec<u8> {
    black
        .as_chunks::<4>()
        .0
        .iter()
        .zip(white.as_chunks::<4>().0.iter())
        .flat_map(|(b, w)| {
            let alpha = 255
                - (0..3)
                    .map(|channel| w[channel].saturating_sub(b[channel]))
                    .max()
                    .unwrap();
            let channel = |index: usize| {
                if alpha == 0 {
                    0
                } else {
                    ((u32::from(b[index]) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                        as u8
                }
            };
            [channel(2), channel(1), channel(0), alpha]
        })
        .collect()
}

use windows::core::{Result as WinResult, PCWSTR};
use windows::Win32::{
    Foundation::{E_FAIL, RPC_E_CHANGED_MODE},
    Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    },
    Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
    UI::{
        Controls::{IImageList, ILD_TRANSPARENT},
        Shell::{
            SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_SYSICONINDEX,
            SHGFI_USEFILEATTRIBUTES, SHIL_JUMBO,
        },
        WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL, HICON},
    },
};

struct ComApartment(bool);
impl ComApartment {
    fn initialize() -> WinResult<Self> {
        // SAFETY: COM is initialized on the same worker thread where all Shell calls run.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result.ok()?;
        Ok(Self(true))
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances our successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}
struct OwnedIcon(HICON);
impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: GetIcon returned this owned handle, which has not been destroyed.
        let _ = unsafe { DestroyIcon(self.0) };
    }
}
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
}
impl Surface {
    fn new() -> WinResult<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ICON_SIZE as i32,
                biHeight: -(ICON_SIZE as i32), // Top-down BGRA.
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: requesting a private memory DC with no borrowed screen DC.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.0.is_null() {
            return Err(windows::core::Error::from_thread());
        }
        let mut bits = std::ptr::null_mut();
        // SAFETY: info is correctly sized; bits receives a pointer owned by the bitmap.
        let bitmap = match unsafe {
            CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        } {
            Ok(bitmap) => bitmap,
            Err(error) => {
                // SAFETY: dc is a live DC owned by this function.
                unsafe {
                    let _ = DeleteDC(dc);
                }
                return Err(error);
            }
        };
        // SAFETY: the live DIB bitmap is compatible with this memory DC.
        let previous = unsafe { SelectObject(dc, HGDIOBJ(bitmap.0)) };
        if previous.0.is_null() || previous.0 as isize == -1 || bits.is_null() {
            let error = windows::core::Error::from_thread();
            // SAFETY: restore any selection before disposing the owned bitmap/DC.
            unsafe {
                if !previous.0.is_null() && previous.0 as isize != -1 {
                    SelectObject(dc, previous);
                }
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
                let _ = DeleteDC(dc);
            }
            return Err(error);
        }
        Ok(Self {
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
        })
    }
    fn draw(&self, icon: HICON, background: u8) -> WinResult<Vec<u8>> {
        let length = ICON_SIZE * ICON_SIZE * 4;
        // SAFETY: the DIB owns length bytes; there is no concurrent access to this DC.
        unsafe { std::slice::from_raw_parts_mut(self.bits, length) }.fill(background);
        // SAFETY: valid owned icon/DC, bounded dimensions, no flicker brush is borrowed.
        unsafe {
            DrawIconEx(
                self.dc,
                0,
                0,
                icon,
                ICON_SIZE as i32,
                ICON_SIZE as i32,
                0,
                None,
                DI_NORMAL,
            )?;
            GdiFlush().ok()?;
        }
        // SAFETY: GdiFlush completed all writes; the DIB allocation remains live.
        Ok(unsafe { std::slice::from_raw_parts(self.bits, length) }.to_vec())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: restore the original object before freeing our private bitmap and DC.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}
fn load_icon(extension: &str, folder: bool) -> WinResult<ShellIcon> {
    let _com = ComApartment::initialize()?;
    let name = if folder || extension.is_empty() {
        "conflux-icon".into()
    } else {
        format!("conflux-icon.{extension}")
    };
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut info = SHFILEINFOW::default();
    // SAFETY: wide is NUL-terminated and info is valid. USEFILEATTRIBUTES prevents file access.
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            if folder {
                FILE_ATTRIBUTE_DIRECTORY
            } else {
                FILE_ATTRIBUTE_NORMAL
            },
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES,
        )
    };
    if result == 0 {
        return Err(windows::core::Error::from_thread());
    }
    // SAFETY: the returned index belongs to the Shell system image lists. The typed COM
    // interface releases its reference automatically; the system list stays read-only.
    let list: IImageList = unsafe { SHGetImageList(SHIL_JUMBO as i32)? };
    let (mut native_width, mut native_height) = (0, 0);
    // SAFETY: valid image list and writable dimension outputs. Never enlarge a smaller list.
    unsafe { list.GetIconSize(&mut native_width, &mut native_height)? };
    if (native_width, native_height) != (ICON_SIZE as i32, ICON_SIZE as i32) {
        return Err(windows::core::Error::new(
            E_FAIL,
            "Shell high-resolution icon list is unavailable",
        ));
    }
    let icon = OwnedIcon(unsafe { list.GetIcon(info.iIcon, ILD_TRANSPARENT.0)? });
    let surface = Surface::new()?;
    let black = surface.draw(icon.0, 0)?;
    let white = surface.draw(icon.0, 255)?;
    Ok(ShellIcon {
        width: ICON_SIZE as u32,
        height: ICON_SIZE as u32,
        rgba: rgba_from_backgrounds(&black, &white),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_input_cannot_be_used_as_a_file_path() {
        assert_eq!(normalize_extension(Some(".PDF".into())).unwrap(), "pdf");
        assert_eq!(normalize_extension(None).unwrap(), "");
        for value in [
            "../exe",
            "file.pdf",
            "C:\\file",
            "pdf\0",
            "..pdf",
            &"a".repeat(33),
        ] {
            assert!(normalize_extension(Some(value.into())).is_err());
        }
    }
    #[test]
    fn transparency_and_bgra_conversion_preserve_colors() {
        assert_eq!(
            rgba_from_backgrounds(&[0, 0, 0, 0], &[255, 255, 255, 0]),
            [0, 0, 0, 0]
        );
        assert_eq!(
            rgba_from_backgrounds(&[10, 20, 30, 0], &[10, 20, 30, 0]),
            [30, 20, 10, 255]
        );
        assert_eq!(
            rgba_from_backgrounds(&[0, 64, 128, 0], &[127, 191, 255, 0]),
            [255, 128, 0, 128]
        );
    }
    #[test]
    fn shell_icons_have_visible_pixels_and_transparent_backgrounds() {
        let mut preview = std::collections::BTreeMap::new();
        for (extension, folder) in [
            ("", false),
            ("iso", false),
            ("png", false),
            ("mp4", false),
            ("mp3", false),
            ("docx", false),
            ("xlsx", false),
            ("pptx", false),
            ("rs", false),
            ("ttf", false),
            ("epub", false),
            ("sqlite", false),
            ("txt", false),
            ("zip", false),
            ("pdf", false),
            ("exe", false),
            ("msi", false),
            ("zst", false),
            ("md", false),
            ("notregistered", false),
            ("", true),
        ] {
            let icon = load_icon(extension, folder).unwrap();
            assert_eq!(
                (icon.width, icon.height),
                (ICON_SIZE as u32, ICON_SIZE as u32)
            );
            assert_eq!(icon.rgba.len(), ICON_SIZE * ICON_SIZE * 4);
            assert!(icon.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
            assert!(icon.rgba.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
            preview.insert(if folder { "folder" } else { extension }, icon);
        }
        // Optional local-only fixture lets the browser dev backend preview this PC's icons.
        if let Some(path) = std::env::var_os("CONFLUX_SHELL_ICON_PREVIEW") {
            std::fs::write(path, serde_json::to_vec(&preview).unwrap()).unwrap();
        }
    }
}
