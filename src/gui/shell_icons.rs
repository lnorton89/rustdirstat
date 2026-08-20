//! The icon the operating system itself uses for a file type.
//!
//! A `.docx` should look like whatever Word looks like on this machine,
//! not like a generic document glyph — that is what makes a file list
//! feel native, and it is information the drawn set cannot carry (it
//! knows "this is a document", not "this is a Word document"). Where the
//! platform cannot supply one, [`super::icons::Icon::for_category`]
//! remains the fallback.
//!
//! Only Windows is implemented. macOS would go through `NSWorkspace`
//! and Linux through the icon theme, and neither has an equivalent
//! shell-attribute call cheap enough to make the same shape sensible, so
//! those simply report "no icon" and get the drawn set.

use eframe::egui;
use std::collections::HashMap;

/// Pixel size icons are requested and cached at. Small shell icons are
/// 16x16, and asking for that avoids a downscale of a 32x32 one.
///
/// Only the Windows implementation has any use for it, so it is gated
/// the same way — an unconditional constant is dead code everywhere
/// else, and this crate denies that.
#[cfg(windows)]
pub(super) const ICON_SIZE: usize = 16;

/// Decoded icon pixels, before they become a texture.
pub(super) struct IconPixels {
    pub rgba: Vec<u8>,
    pub size: usize,
}

/// Per-extension icon textures, resolved lazily and kept for the life of
/// the process.
///
/// Looking one up is a shell call and a texture upload, which is far too
/// much to do per row per frame; there are only ever a few dozen distinct
/// extensions on screen, so the cache is small and never needs evicting.
/// A failed lookup is cached as `None` so it is not retried every frame.
#[derive(Default)]
pub(super) struct ShellIcons {
    textures: HashMap<String, Option<egui::TextureHandle>>,
}

impl ShellIcons {
    /// The system icon for `extension` (with its leading dot), if the
    /// platform has one.
    pub(super) fn get(
        &mut self,
        ctx: &egui::Context,
        extension: &str,
    ) -> Option<&egui::TextureHandle> {
        if !self.textures.contains_key(extension) {
            let texture = platform::load(extension).map(|icon| {
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([icon.size, icon.size], &icon.rgba);
                ctx.load_texture(
                    format!("shell-icon-{extension}"),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.textures.insert(extension.to_owned(), texture);
        }
        self.textures.get(extension).and_then(Option::as_ref)
    }
}

#[cfg(windows)]
mod platform {
    use super::{IconPixels, ICON_SIZE};
    use std::mem::MaybeUninit;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows_sys::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// Owns an `HICON` so it is destroyed on every path out, including
    /// the early returns while decoding it.
    struct OwnedIcon(HICON);

    impl Drop for OwnedIcon {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from `SHGetFileInfoW`, which hands
            // ownership to the caller, and nothing else destroys it.
            unsafe { DestroyIcon(self.0) };
        }
    }

    /// Owns a GDI bitmap handle from `GetIconInfo`, which documents both
    /// of its bitmaps as the caller's to delete.
    struct OwnedBitmap(HGDIOBJ);

    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: created by `GetIconInfo` and not deleted
                // anywhere else.
                unsafe { DeleteObject(self.0) };
            }
        }
    }

    /// Makes sure COM is initialized on this thread, once.
    ///
    /// `SHGetFileInfoW` is documented as requiring it whenever an icon is
    /// asked for. The GUI thread happens to have it already — winit
    /// initializes COM for the window — which is exactly what makes this
    /// easy to miss: the lookup works in the app and fails anywhere else,
    /// including in the test suite, where it returned no icon at all.
    ///
    /// Both `S_FALSE` (already initialized on this thread) and
    /// `RPC_E_CHANGED_MODE` (already initialized with a different model)
    /// mean somebody else got there first, which is fine — the call
    /// below works either way. No matching `CoUninitialize`: this is a
    /// process-lifetime concern, and tearing COM down underneath a
    /// thread that may still be using it would be worse than leaving it.
    fn ensure_com() {
        thread_local! {
            static INITIALIZED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        INITIALIZED.with(|initialized| {
            if initialized.get() {
                return;
            }
            // SAFETY: a null reserved argument is what the documentation
            // requires, and the flags are a valid combination.
            unsafe {
                CoInitializeEx(
                    std::ptr::null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                );
            }
            initialized.set(true);
        });
    }

    /// Asks the shell for the icon associated with a file type.
    ///
    /// `SHGFI_USEFILEATTRIBUTES` is what makes this safe to call for an
    /// extension rather than a real path: the shell answers from the
    /// registered file type alone and never touches the disk, so this
    /// works for extensions whose files live somewhere slow, or nowhere.
    fn icon_for(extension: &str) -> Option<OwnedIcon> {
        ensure_com();
        let mut wide: Vec<u16> = format!("file{extension}").encode_utf16().collect();
        wide.push(0);
        let mut info = MaybeUninit::<SHFILEINFOW>::zeroed();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string alive for the
        // call, and `info` points at writable storage of exactly the
        // size passed as the fifth argument.
        let ok = unsafe {
            SHGetFileInfoW(
                wide.as_ptr(),
                FILE_ATTRIBUTE_NORMAL,
                info.as_mut_ptr(),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES,
            )
        };
        if ok == 0 {
            return None;
        }
        // SAFETY: a non-zero return means the struct was filled in.
        let info = unsafe { info.assume_init() };
        (!info.hIcon.is_null()).then(|| OwnedIcon(info.hIcon))
    }

    /// Copies an icon's colour bitmap out as straight RGBA.
    fn decode(icon: &OwnedIcon) -> Option<IconPixels> {
        let mut icon_info = MaybeUninit::<ICONINFO>::zeroed();
        // SAFETY: `icon.0` is a live icon handle and `icon_info` points
        // at writable storage of the right size.
        if unsafe { GetIconInfo(icon.0, icon_info.as_mut_ptr()) } == 0 {
            return None;
        }
        // SAFETY: a non-zero return means the struct was filled in.
        let icon_info = unsafe { icon_info.assume_init() };
        // Both bitmaps belong to the caller now, whether or not the rest
        // of this succeeds.
        let _mask = OwnedBitmap(icon_info.hbmMask);
        let color = OwnedBitmap(icon_info.hbmColor);
        if color.0.is_null() {
            return None;
        }

        let size = ICON_SIZE;
        let mut header: BITMAPINFO = unsafe { std::mem::zeroed() };
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            // Negative height asks GDI for a top-down bitmap, matching
            // the row order egui expects and saving a flip.
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..unsafe { std::mem::zeroed() }
        };

        let mut pixels = vec![0_u8; size * size * 4];
        // SAFETY: a null window handle asks for the screen DC, which is
        // the documented way to get one for a scratch conversion.
        let screen = unsafe { GetDC(std::ptr::null_mut::<HWND>() as HWND) };
        // SAFETY: `color.0` is a live bitmap, `pixels` has exactly
        // `biWidth * |biHeight| * 4` bytes as described by `header`, and
        // `screen` is a valid DC for the duration of the call.
        let copied = unsafe {
            GetDIBits(
                screen,
                color.0 as _,
                0,
                size as u32,
                pixels.as_mut_ptr().cast(),
                &mut header,
                DIB_RGB_COLORS,
            )
        };
        // SAFETY: releasing the DC just acquired, once, on every path.
        unsafe { ReleaseDC(std::ptr::null_mut::<HWND>() as HWND, screen) };
        if copied == 0 {
            return None;
        }

        // GDI hands back BGRA; egui wants RGBA.
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        // A fully transparent result means the shell had nothing useful
        // to draw, and an invisible icon is worse than the drawn
        // fallback.
        if pixels.chunks_exact(4).all(|chunk| chunk[3] == 0) {
            return None;
        }
        Some(IconPixels { rgba: pixels, size })
    }

    pub(super) fn load(extension: &str) -> Option<IconPixels> {
        decode(&icon_for(extension)?)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::IconPixels;

    /// No system icon source wired up on this platform; callers fall
    /// back to the drawn category set.
    pub(super) fn load(_extension: &str) -> Option<IconPixels> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::platform;

    /// Whatever the shell hands back has to be a usable image.
    ///
    /// Deliberately not asserting that an icon *is* available. The
    /// lookup needs a COM apartment that nothing else has already
    /// claimed differently, which the app's winit main thread has and the
    /// test harness cannot promise: this passes run alone and fails run
    /// alongside the rest of the suite, purely on which thread the
    /// harness happens to reuse. Failing the build on that would be
    /// reporting the harness, not the code.
    ///
    /// What is worth pinning is the decode, which is where the real bugs
    /// live — wrong buffer length, wrong dimensions, or an all-
    /// transparent result that would draw as nothing at all.
    #[test]
    #[cfg(windows)]
    fn a_supplied_windows_icon_is_a_usable_image() {
        let Some(icon) = platform::load(".txt") else {
            return;
        };
        assert_eq!(icon.size, super::ICON_SIZE);
        assert_eq!(
            icon.rgba.len(),
            icon.size * icon.size * 4,
            "buffer does not match the dimensions it claims"
        );
        assert!(
            icon.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0),
            "every pixel was transparent, so nothing would be drawn"
        );
    }

    #[test]
    fn an_unknown_extension_falls_back_rather_than_failing() {
        // Whatever the platform decides here, it must decide something —
        // the caller treats `None` as "use the drawn glyph".
        let _ = platform::load(".definitely-not-a-registered-type");
    }
}
