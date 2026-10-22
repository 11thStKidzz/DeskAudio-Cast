//! Only app-owned daily logs; never recurse, follow links, or remove an open writer.
use std::{fs, path::Path, time::{Duration, SystemTime}};
pub const RETAIN_DAYS: u64 = 14;
pub const MAX_FILES: usize = 14;

fn owned_name(name: &str) -> bool {
    let Some(date) = name.strip_prefix("app.log.") else { return false; };
    date.len() == 10 && date.bytes().enumerate().all(|(i,b)|
        if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

pub fn prune(dir: &Path, now: SystemTime) -> usize {
    if fs::symlink_metadata(dir).map_or(true, |m| m.file_type().is_symlink() || !m.is_dir()) { return 0; }
    let Ok(entries) = fs::read_dir(dir) else { return 0; };
    let mut files: Vec<_> = entries.flatten().filter_map(|e| {
        if !owned_name(e.file_name().to_str()?) { return None; }
        let m = fs::symlink_metadata(e.path()).ok()?;
        if !m.is_file() || m.file_type().is_symlink() { return None; }
        Some((e.path(), m.modified().ok()?))
    }).collect();
    files.sort_by_key(|(_,time)| std::cmp::Reverse(*time));
    let mut removed = 0;
    for (index,(path,modified)) in files.iter().enumerate() {
        // Keep recently touched files even if another process has temporarily closed it.
        let age = now.duration_since(*modified).unwrap_or_default();
        if age < Duration::from_secs(24*60*60) { continue; }
        if index < MAX_FILES && age < Duration::from_secs(RETAIN_DAYS*24*60*60) { continue; }
        #[cfg(windows)] {
            use std::os::windows::fs::OpenOptionsExt;
            // DELETE_ON_CLOSE + exclusive share: no gap between lock check and removal.
            if fs::OpenOptions::new().read(true).access_mode(0x80000000 | 0x00010000)
                .share_mode(0).custom_flags(0x04000000).open(path).is_ok() { removed += 1; }
        }
        #[cfg(not(windows))] { let _ = path; } // Windows release is the supported target.
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn strict_ownership_and_no_recursive_matches() {
        for name in ["app.log.2026-10-01","app.log.2000-01-01"] { assert!(owned_name(name)); }
        for name in ["other.log.2026-10-01","app.log.2026-10-01.zip","app.log", "../app.log.2026-10-01"] { assert!(!owned_name(name)); }
    }
    #[test] fn closed_old_log_removed_unrelated_and_open_log_preserved() {
        let root = std::env::temp_dir().join(format!("desk-retention-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let old = root.join("app.log.2000-01-01");
        let active = root.join("app.log.2000-01-02");
        let other = root.join("user-notes.txt");
        for p in [&old,&active,&other] { fs::write(p,b"synthetic fixture").unwrap(); }
        let handle = fs::File::open(&active).unwrap();
        let future = SystemTime::now()+Duration::from_secs(30*86400);
        #[cfg(windows)] { assert_eq!(prune(&root,future),1); assert!(!old.exists()); }
        assert!(active.exists()); assert!(other.exists());
        drop(handle);
        fs::remove_dir_all(root).unwrap();
    }
}
