//! One instance per interactive Windows session, independent of EXE location.
//! Kernel objects use the caller's default DACL; no socket, file, port or payload.
#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, sync::Arc};
    #[link(name="kernel32")]
    extern "system" {
        fn CreateMutexW(sa:*const c_void, owner:i32, name:*const u16)->*mut c_void;
        fn CreateEventW(sa:*const c_void, manual:i32, initial:i32, name:*const u16)->*mut c_void;
        fn GetLastError()->u32;
        fn SetEvent(event:*mut c_void)->i32;
        fn WaitForSingleObject(handle:*mut c_void, millis:u32)->u32;
        fn CloseHandle(handle:*mut c_void)->i32;
    }
    struct Handle(isize);
    impl Drop for Handle { fn drop(&mut self){unsafe {CloseHandle(self.0 as *mut c_void);}} }
    pub struct Primary { _mutex:Handle, event:Arc<Handle> }
    pub enum Instance { Primary(Primary), Secondary }
    fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
    pub fn acquire_named(base:&str)->Result<Instance, &'static str>{
        let mutex=unsafe {CreateMutexW(std::ptr::null(),0,wide(&format!("Local\\{base}.mutex")).as_ptr())};
        if mutex.is_null(){return Err("single_instance_unavailable")}
        let exists=unsafe {GetLastError()}==183;
        let mutex=Handle(mutex as isize);
        let event=unsafe {CreateEventW(std::ptr::null(),0,0,wide(&format!("Local\\{base}.activate")).as_ptr())};
        if event.is_null(){return Err("single_instance_unavailable")}
        let event=Arc::new(Handle(event as isize));
        if exists {
            if unsafe {SetEvent(event.0 as *mut c_void)}==0{return Err("single_instance_activation_failed")}
            return Ok(Instance::Secondary)
        }
        Ok(Instance::Primary(Primary{_mutex:mutex,event}))
    }
    impl Primary {
        pub fn on_activate(&self, callback:impl Fn()+Send+'static){
            let event=self.event.clone();
            std::thread::spawn(move || loop {
                if unsafe {WaitForSingleObject(event.0 as *mut c_void,u32::MAX)}!=0 {break}
                callback();
            });
        }
        #[cfg(test)] fn notified(&self)->bool {unsafe {WaitForSingleObject(self.event.0 as *mut c_void,3000)==0}}
    }
    pub fn acquire()->Result<Instance,&'static str>{acquire_named("DeskAudioCast.desktop.v1")}
    #[cfg(test)] mod tests {
        use super::*;
        fn name()->String{format!("DeskAudioCast.test.{}.{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())}
        #[test] fn second_instance_signals_instead_of_starting(){
            let name=name();let Instance::Primary(first)=acquire_named(&name).unwrap() else {panic!()};
            assert!(matches!(acquire_named(&name).unwrap(),Instance::Secondary));assert!(first.notified());
        }
        #[test] fn process_exit_or_drop_releases_the_name(){
            let name=name();let first=acquire_named(&name).unwrap();drop(first);
            assert!(matches!(acquire_named(&name).unwrap(),Instance::Primary(_)));
        }
        #[test] fn activation_works_across_real_processes(){
            let name=name();let Instance::Primary(first)=acquire_named(&name).unwrap() else {panic!()};
            let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","single_instance::platform::tests::secondary_process_probe","--nocapture"]).env("DESKAUDIO_TEST_INSTANCE",&name).status().unwrap();
            assert!(status.success());assert!(first.notified());
        }
        #[test] fn secondary_process_probe(){
            if let Ok(name)=std::env::var("DESKAUDIO_TEST_INSTANCE"){assert!(matches!(acquire_named(&name).unwrap(),Instance::Secondary));}
        }
    }
}
#[cfg(windows)] pub use platform::{acquire,Instance};
