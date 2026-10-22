//! Local, fixed documents only. No URL or filesystem path supplied by the WebView.
#[tauri::command]
pub fn get_legal_document(kind: String) -> Result<String, String> {
    let text = match kind.as_str() {
        "about" => include_str!("../../legal/ABOUT.txt"),
        "privacy" => include_str!("../../legal/PRIVACY.txt"),
        "licenses" => concat!(include_str!("../../legal/OPEN_SOURCE.txt"), "\n\n", include_str!("../../LICENSE")),
        "third_party" => concat!(include_str!("../../legal/THIRD_PARTY_NOTICES.txt"), "\n\n", include_str!("../../legal/third-party/LICENSE_TEXTS.txt"), "\n\nMicrosoft WebView2 SDK Loader 1.0.3650.58\n", include_str!("../../legal/native/WebView2-SDK-1.0.3650.58-LICENSE.txt"), "\n\nNSIS 3.11 (installer components)\n", include_str!("../../legal/native/NSIS-3.11-COPYING.txt"), "\n\nMicrosoft WebView2 Fixed Runtime — separate Microsoft terms\n", include_str!("../../legal/native/WebView2-Fixed-Runtime-EULA.txt")),
        "support" => include_str!("../../legal/SUPPORT.txt"),
        _ => return Err("unknown_document".into()),
    };
    Ok(text.into())
}
#[tauri::command]
pub fn get_build_info() -> serde_json::Value {
    serde_json::json!({
        "version":env!("CARGO_PKG_VERSION"),
        "revision":option_env!("DESKAUDIO_BUILD_REVISION"),
        "source_fingerprint":option_env!("DESKAUDIO_SOURCE_FINGERPRINT"),
        "distribution_status":"store",
        "webview2_deployment":"fixed_app_local",
        "webview2_version":crate::runtime_prerequisites::FIXED_RUNTIME_VERSION
    })
}
#[cfg(test)]
mod tests {
    #[test] fn public_support_is_present_in_fixed_documents() {
        for key in ["about", "privacy", "support"] {
            let body = super::get_legal_document(key.into()).unwrap();
            assert!(body.contains("11thStKidzz"));
            assert!(body.contains("woyaotingspotify@gmail.com"));
        }
    }
    #[test] fn only_fixed_local_documents_are_available() {
        for key in ["about","privacy","licenses","third_party","support"] {
            assert!(!super::get_legal_document(key.into()).unwrap().trim().is_empty());
        }
        for bad in ["../settings.json","https://example.com","", "C:\\private"] {
            assert!(super::get_legal_document(bad.into()).is_err());
        }
    }
}
