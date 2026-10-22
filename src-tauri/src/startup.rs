//! Current-user login startup for unpackaged Windows builds. No admin rights.
//! Packaged builds use the manifest's opt-in StartupTask, never the Run entry.
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    type Hkey = *mut c_void;
    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const NAME: &str = "DeskAudioCast";
    #[link(name = "advapi32")]
    extern "system" {
        fn RegCreateKeyExW(
            key: Hkey,
            sub: *const u16,
            reserved: u32,
            class: *mut u16,
            options: u32,
            access: u32,
            security: *const c_void,
            result: *mut Hkey,
            disposition: *mut u32,
        ) -> i32;
        fn RegOpenKeyExW(
            key: Hkey,
            sub: *const u16,
            options: u32,
            access: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegSetValueExW(
            key: Hkey,
            name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            len: u32,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            len: *mut u32,
        ) -> i32;
        fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentPackageFullName(len: *mut u32, name: *mut u16) -> i32;
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    struct Key(Hkey);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    pub fn packaged() -> bool {
        let mut len = 0;
        // APPMODEL_ERROR_NO_PACKAGE = 15700. Fail closed on unexpected errors.
        unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) != 15700 }
    }
    fn open(access: u32, create: bool) -> Result<Option<Key>, String> {
        let mut key = std::ptr::null_mut();
        let mut status = unsafe {
            RegOpenKeyExW(
                0x80000001u32 as i32 as isize as Hkey,
                wide(KEY).as_ptr(),
                0,
                access,
                &mut key,
            )
        };
        if status == 2 && create {
            // A clean profile may have no Run key. Creation happens only after opt-in.
            status = unsafe {
                RegCreateKeyExW(
                    0x80000001u32 as i32 as isize as Hkey,
                    wide(KEY).as_ptr(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    access,
                    std::ptr::null(),
                    &mut key,
                    std::ptr::null_mut(),
                )
            };
        }
        if status == 2 {
            return Ok(None);
        }
        if status != 0 {
            return Err(format!("startup_registry_error:{status}"));
        }
        Ok(Some(Key(key)))
    }
    pub fn enabled() -> Result<bool, String> {
        if packaged() {
            return super::packaged_startup::enabled();
        }
        let Some(key) = open(0x0001, false)? else {
            return Ok(false);
        }; // KEY_QUERY_VALUE
        let mut data = [0u16; 32768];
        let mut size = (data.len() * 2) as u32;
        let mut kind = 0;
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                wide(NAME).as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status == 2 {
            return Ok(false);
        }
        if status != 0 {
            return Err(format!("startup_registry_error:{status}"));
        }
        let value = String::from_utf16_lossy(&data[..(size as usize / 2).saturating_sub(1)]);
        Ok(kind == 1
            && value == super::command(&std::env::current_exe().map_err(|e| e.to_string())?)?)
    }
    pub fn set(enabled: bool) -> Result<(), String> {
        if packaged() {
            return super::packaged_startup::set(enabled);
        }
        let Some(key) = open(0x0002, enabled)? else {
            return Ok(());
        }; // KEY_SET_VALUE; only our named value is changed.
        let status = if enabled {
            let command = wide(&super::command(
                &std::env::current_exe().map_err(|e| e.to_string())?,
            )?);
            unsafe {
                RegSetValueExW(
                    key.0,
                    wide(NAME).as_ptr(),
                    0,
                    1,
                    command.as_ptr().cast(),
                    (command.len() * 2) as u32,
                )
            }
        } else {
            unsafe { RegDeleteValueW(key.0, wide(NAME).as_ptr()) }
        };
        if status != 0 && !(status == 2 && !enabled) {
            return Err(format!("startup_registry_error:{status}"));
        }
        Ok(())
    }
}

fn command(path: &std::path::Path) -> Result<String, String> {
    let path = path.to_str().ok_or("startup_invalid_path")?;
    if path.contains(['"', '\0', '\n', '\r']) {
        return Err("startup_invalid_path".into());
    }
    Ok(format!("\"{path}\" --background"))
}
pub fn supported() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod packaged_startup {
    use ::windows::ApplicationModel::{StartupTask, StartupTaskState};
    fn task() -> Result<StartupTask,String> {
        StartupTask::GetAsync(&"DeskAudioCastStartup".into())
            .and_then(|op|op.get()).map_err(|_|"startup_packaged_pending".into())
    }
    pub fn enabled() -> Result<bool,String> {
        let state=task()?.State().map_err(|_|"startup_packaged_pending")?;
        Ok(state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy)
    }
    pub fn set(enable: bool) -> Result<(),String> {
        let task=task()?;
        if !enable { return task.Disable().map_err(|_|"startup_windows_blocked".into()); }
        let state=task.RequestEnableAsync().and_then(|op|op.get())
            .map_err(|_|"startup_windows_blocked")?;
        // Never override a user's Task Manager choice or an administrator's policy.
        if state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy { Ok(()) }
        else { Err("startup_windows_blocked".into()) }
    }
}
pub fn enabled() -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows::enabled()
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}
pub fn set(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::set(enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("startup_windows_only".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_paths_with_spaces_and_adds_background_flag() {
        assert_eq!(
            command(std::path::Path::new("C:\\Some App\\桌面声渡.exe")).unwrap(),
            "\"C:\\Some App\\桌面声渡.exe\" --background"
        );
    }
    #[test]
    fn rejects_command_injection() {
        for path in ["a\" --bad", "a\0b", "a\nb", "a\rb"] {
            assert!(command(std::path::Path::new(path)).is_err());
        }
    }
}
