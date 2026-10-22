//! Select app-local Fixed Runtime before creating a WebView. The custom MSIX
//! and NSIS copy this folder; Tauri bundler installation is deliberately skipped.
pub const FIXED_RUNTIME_VERSION: &str = "155.0.4283.62";
pub const FIXED_RUNTIME_FOLDER: &str = "WebView2Fixed";

fn fixed_runtime_ready(app_dir: &std::path::Path) -> bool {
    let runtime=app_dir.join(FIXED_RUNTIME_FOLDER);
    ["msedgewebview2.exe".to_string(), "msedge.dll".into(), "icudtl.dat".into(),
        format!("{FIXED_RUNTIME_VERSION}.manifest")].iter().all(|name| {
        std::fs::metadata(runtime.join(name)).is_ok_and(|meta|meta.is_file() && meta.len()>0)
    })
}

pub fn show_startup_failure(code: &str) {
    // Bounded non-sensitive record: never store raw OS errors, paths or audio.
    let code=match code {"fixed_runtime_missing"=>"fixed_runtime_missing",_=>"webview_initialization_failed"};
    if let Some(local)=std::env::var_os("LOCALAPPDATA") {
        let dir=std::path::PathBuf::from(local).join("com.personal.desktopaudiotransmitter/diagnostics");
        if std::fs::create_dir_all(&dir).is_ok() {
            let _=std::fs::write(dir.join("deployment-error.txt"),
                format!("DeskAudio Cast {}\ncode={code}\nfixed_runtime={FIXED_RUNTIME_VERSION}\n",env!("CARGO_PKG_VERSION")));
        }
    }
    #[cfg(windows)] {
        #[link(name="user32")]
        unsafe extern "system" {fn MessageBoxW(window:isize,message:*const u16,title:*const u16,flags:u32)->i32;}
        let wide=|s:&str|s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let message=wide(&format!("内置 WebView2 运行时缺失、不完整或无法启动。请重新安装完整的桌面声渡安装包，不要只复制 EXE。若仍失败，请联系支持并提供以下诊断文件。无需另行安装 WebView2。\n\nThe bundled WebView2 Runtime is missing, incomplete or could not start. Reinstall the complete DeskAudio Cast package; do not copy only the EXE. If this continues, send the diagnostic file to support. A separate WebView2 installation is not required.\n\n%LOCALAPPDATA%\\com.personal.desktopaudiotransmitter\\diagnostics\\deployment-error.txt\ncode={code}\nwoyaotingspotify@gmail.com"));
        unsafe {MessageBoxW(0,message.as_ptr(),wide("桌面声渡 / DeskAudio Cast").as_ptr(),0x10);}
    }
}

#[cfg(windows)]
pub fn ensure_webview_runtime() -> bool {
    if let Some(dir)=std::env::current_exe().ok().and_then(|p|p.parent().map(std::path::Path::to_path_buf)) {
        if fixed_runtime_ready(&dir) {
            // Called after acquiring kernel instance handles, but BEFORE tracing,
            // the activation listener, Tokio or WebView spawn any worker threads.
            // Wry passes a null browser folder to WebView2; Microsoft's loader
            // honors this official environment override for MSIX and Win32 alike.
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",dir.join(FIXED_RUNTIME_FOLDER));
            return true;
        }
    }
    show_startup_failure("fixed_runtime_missing");
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_partial_empty_or_wrong_version_runtime() {
        let root=std::env::temp_dir().join(format!("deskaudio-runtime-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(root.join(FIXED_RUNTIME_FOLDER)).unwrap();
        assert!(!fixed_runtime_ready(&root));
        for name in ["msedgewebview2.exe","msedge.dll","icudtl.dat","wrong.manifest"] {
            std::fs::write(root.join(FIXED_RUNTIME_FOLDER).join(name),b"fixture").unwrap();
        }
        assert!(!fixed_runtime_ready(&root));
        let manifest=root.join(FIXED_RUNTIME_FOLDER).join(format!("{FIXED_RUNTIME_VERSION}.manifest"));
        std::fs::write(&manifest,b"").unwrap();assert!(!fixed_runtime_ready(&root));
        std::fs::write(&manifest,b"fixture").unwrap();assert!(fixed_runtime_ready(&root));
        // Only this test's uniquely-created directory, never user data.
        std::fs::remove_dir_all(root).unwrap();
    }
}
