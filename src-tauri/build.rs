fn main() {
    println!("cargo:rerun-if-env-changed=DESKAUDIO_LINK_MAP");
    if let Some(map)=std::env::var_os("DESKAUDIO_LINK_MAP") {
        let map=std::path::PathBuf::from(map);
        assert!(map.is_absolute(),"Link map must have an absolute path");
        let text=map.to_str().expect("Link map path must be Unicode");
        assert!(!text.contains(['\n','\r']),"Invalid link map path");
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            println!("cargo:rustc-link-arg-bin=conexion-airplay=/MAP:{text}");
        }
    }
    println!("cargo:rerun-if-env-changed=DESKAUDIO_BUILD_REVISION");
    println!("cargo:rerun-if-env-changed=DESKAUDIO_SOURCE_FINGERPRINT");
    if !cfg!(debug_assertions) {
        for key in ["DESKAUDIO_BUILD_REVISION", "DESKAUDIO_SOURCE_FINGERPRINT"] {
            let value = std::env::var(key).expect("Release must use tools/validate_local.ps1 for traceable source identity");
            assert!(!value.is_empty() && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-'), "Invalid build identity");
        }
    }
    tauri_build::build()
}
