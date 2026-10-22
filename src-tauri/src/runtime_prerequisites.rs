//! Fail visibly before creating a WebView if its separate Runtime is absent.
//! Read only Microsoft's documented Evergreen registration, never install/download.
fn valid_version(value: &str) -> bool {
    let parts = value.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>();
    matches!(parts, Ok(v) if v.len() == 4 && v.iter().any(|part| *part != 0))
}

#[cfg(windows)]
pub fn ensure_webview_runtime() -> bool {
    use std::ffi::c_void;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(key: isize, subkey: *const u16, value: *const u16, flags: u32,
            kind: *mut u32, data: *mut c_void, bytes: *mut u32) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: isize, message: *const u16, title: *const u16, flags: u32) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let key = wide("Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}");
    let name = wide("pv");
    // HKCU per-user; HKLM's 32-bit registry view for per-machine x64 Windows.
    for (root, view) in [(0x80000001u32, 0), (0x80000002u32, 0x20000)] {
        let mut data = [0u16; 256];
        let mut bytes = std::mem::size_of_val(&data) as u32;
        let status = unsafe { RegGetValueW(root as i32 as isize, key.as_ptr(), name.as_ptr(),
            2 | view, std::ptr::null_mut(), data.as_mut_ptr().cast(), &mut bytes) };
        if status == 0 {
            let end = data.iter().position(|v| *v == 0).unwrap_or(data.len());
            if valid_version(&String::from_utf16_lossy(&data[..end])) { return true; }
        }
    }
    let message = wide("需要 Microsoft Edge WebView2 Runtime 才能显示界面。请从微软官方网站安装后重新打开；本应用不会自动下载或安装组件。\n\nMicrosoft Edge WebView2 Runtime is required. Install it from Microsoft's official website, then reopen the app. No component is downloaded or installed automatically.\n\nhttps://developer.microsoft.com/microsoft-edge/webview2/");
    let title = wide("桌面声渡 / DeskAudio Cast");
    unsafe { MessageBoxW(0, message.as_ptr(), title.as_ptr(), 0x10); }
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_missing_zero_or_malformed_runtime_registration() {
        for version in ["", "0.0.0.0", "123", "123.0.bad.0", "1.2.3.4.5", "-1.2.3.4"] {
            assert!(!super::valid_version(version));
        }
        assert!(super::valid_version("141.0.3537.71"));
    }
}
