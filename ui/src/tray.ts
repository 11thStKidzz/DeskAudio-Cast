import "./tray.css";
import {invoke} from "@tauri-apps/api/core";
import {listen} from "@tauri-apps/api/event";
import {VisiblePoller} from "./visible-poller";
import {LatestVolume} from "./latest-volume";
import {setLang,t,type Lang} from "./i18n";
import {transportControl} from "./tray-controls.mjs";
type TrayState={playing:boolean;busy:boolean;volume:number;can_resume:boolean;language:Lang;show_label:string;quit_label:string;visible:boolean};
const volume=document.getElementById("tray-volume") as HTMLInputElement;
const percent=document.getElementById("tray-percent")!;
const transport=document.getElementById("tray-transport") as HTMLButtonElement;
const error=document.getElementById("tray-error")!;
let action=false;let editingUntil=0;let debounce:ReturnType<typeof setTimeout>|undefined;
function apply(s:TrayState){
  setLang(s.language);document.getElementById("tray-show")!.textContent=s.show_label;document.getElementById("tray-quit")!.textContent=s.quit_label;
  volume.setAttribute("aria-label",t("volume"));
  if(Date.now()>editingUntil){volume.value=String(Math.round(s.volume*100));percent.textContent=`${volume.value}%`;}
  const control=transportControl(s,action);transport.textContent=control.glyph;
  const label=t(s.playing?"stop":"play_generic").replace(/^[▶⏸]\s*/,"");transport.title=label;transport.setAttribute("aria-label",label);
  transport.disabled=control.disabled;volume.disabled=action||s.busy;
}
const poller=new VisiblePoller(()=>invoke<TrayState>("get_tray_state"),apply,650,1800);
const changes=new LatestVolume(async value=>{await invoke("tray_set_volume",{volume:value});},e=>{error.textContent=t("vol_error_prefix",{err:String(e)});});
volume.addEventListener("input",()=>{editingUntil=Date.now()+1800;percent.textContent=`${volume.value}%`;if(debounce)clearTimeout(debounce);debounce=setTimeout(()=>changes.set(Number(volume.value)/100),100);});
transport.addEventListener("click",()=>{if(action)return;action=true;transport.disabled=true;error.textContent="";void invoke("tray_toggle_stream").catch(e=>{error.textContent=t(String(e));}).finally(()=>{action=false;void invoke<TrayState>("get_tray_state").then(apply);});});
document.getElementById("tray-show")!.addEventListener("click",()=>{void invoke("tray_show_main");});
document.getElementById("tray-quit")!.addEventListener("click",()=>{void invoke("tray_quit");});
void listen<boolean>("app://tray-visible",e=>{poller.setVisible(e.payload);if(!e.payload)error.textContent="";});
void listen("app://tray-refresh",()=>{void invoke<TrayState>("get_tray_state").then(apply);});
void invoke<TrayState>("get_tray_state").then(s=>{apply(s);poller.setVisible(s.visible);});
window.addEventListener("pagehide",()=>{poller.dispose();if(debounce)clearTimeout(debounce);},{once:true});
