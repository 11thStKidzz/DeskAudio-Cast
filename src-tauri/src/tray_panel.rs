//! A fixed local popup, not a native menu with unsupported custom controls.
use tauri::{Emitter,Manager};

pub fn setup(app:&tauri::AppHandle)->tauri::Result<()> {
    let panel=tauri::WebviewWindowBuilder::new(app,"tray-panel",tauri::WebviewUrl::App("tray.html".into()))
        .title("DeskAudio Cast controls").inner_size(300.0,154.0).min_inner_size(300.0,154.0)
        .decorations(false).resizable(false).skip_taskbar(true).always_on_top(true).visible(false).build()?;
    let handle=app.clone();
    panel.on_window_event(move|event|{
        if let tauri::WindowEvent::CloseRequested{api,..}=event{api.prevent_close();}
        if matches!(event,tauri::WindowEvent::Focused(false)|tauri::WindowEvent::CloseRequested{..}){
            if let Some(panel)=handle.get_webview_window("tray-panel") {let _=panel.hide();let _=panel.emit("app://tray-visible",false);}
        }
    });
    Ok(())
}
pub fn show(app:&tauri::AppHandle,anchor:tauri::PhysicalPosition<f64>){
    let Some(panel)=app.get_webview_window("tray-panel") else{return};
    if let Ok(Some(monitor))=app.monitor_from_point(anchor.x,anchor.y){
        let area=monitor.work_area();let scale=monitor.scale_factor();
        let width=(300.0*scale).ceil() as i32;let height=(154.0*scale).ceil() as i32;
        let (x,y)=clamp_position(anchor.x as i32,anchor.y as i32,width,height,area.position.x,area.position.y,area.size.width as i32,area.size.height as i32);
        let _=panel.set_position(tauri::PhysicalPosition::new(x,y));
    }
    let _=panel.show();let _=panel.set_focus();let _=panel.emit("app://tray-visible",true);
}
pub fn hide(app:&tauri::AppHandle){if let Some(panel)=app.get_webview_window("tray-panel"){let _=panel.hide();let _=panel.emit("app://tray-visible",false);}}
fn clamp_position(x:i32,y:i32,w:i32,h:i32,left:i32,top:i32,mw:i32,mh:i32)->(i32,i32){
    ((x-w).clamp(left,(left+mw-w).max(left)),(y-h).clamp(top,(top+mh-h).max(top)))
}
#[cfg(test)]mod tests {
    #[test]fn popup_stays_on_monitor_including_negative_coordinates(){
        assert_eq!(super::clamp_position(1910,1070,300,154,0,0,1920,1040),(1610,886));
        assert_eq!(super::clamp_position(-1910,-5,300,154,-1920,0,1920,1040),(-1920,0));
    }
}
