import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { groupDevices, routesFor, type Device, type DeviceKind } from "./devices";
import { VisiblePoller } from "./visible-poller";
import { LatestVolume } from "./latest-volume";
import {
  applyStaticTranslations,
  getLang,
  onLangChange,
  t,
  setLang,
  type Lang,
} from "./i18n";

const KIND_LABEL: Record<DeviceKind, string> = {
  homepod: "HomePod",
  appletv: "Apple TV",
  airportexpress: "AirPort Express",
  otherairplay: "AirPlay",
};

const devicesList = document.getElementById("devices") as HTMLUListElement;
const scanBtn = document.getElementById("scan") as HTMLButtonElement;
const statusEl = document.getElementById("status") as HTMLSpanElement;
const toastEl = document.getElementById("toast") as HTMLDivElement;
const connectionResult = document.getElementById("connection-result") as HTMLDivElement;
const connectionMessage = document.getElementById("connection-message") as HTMLParagraphElement;
const connectionDetails = document.getElementById("connection-details") as HTMLDetailsElement;
const connectionError = document.getElementById("connection-error") as HTMLPreElement;

// Keep connection feedback separate from the discovery count updated by render().
function showConnectionResult(message: string, error?: unknown) {
  connectionResult.hidden = false;
  connectionMessage.dataset.i18n = message;
  connectionMessage.textContent = t(message);
  connectionDetails.hidden = error === undefined;
  connectionError.textContent = error === undefined ? "" : String(error).slice(0, 1500);
  connectionDetails.open = error !== undefined;
}

let toastTimer: number | null = null;
function showToast(msg: string, durationMs = 5000) {
  toastEl.textContent = msg;
  toastEl.hidden = false;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toastEl.hidden = true;
    toastTimer = null;
  }, durationMs);
}

// Errores asíncronos del backend (pump muriendo, heartbeat fallando reiteradamente).
// Los errores síncronos de invoke se siguen mostrando junto a su botón asociado.
// La función se llama al final del archivo, una vez declarados `playing` y demás.
function setupAsyncErrorListener() {
  void listen<{generation: number; code: string}>("airplay://mode-fallback", (event) => {
    if (event.payload.generation !== activeGeneration) return;
    showToast(t("low_buffer_unstable"), 8000);
    void preloadSavedLatency();
  });
  void listen<{generation: number; code: string}>("airplay://error", async (event) => {
    if (event.payload.generation !== activeGeneration) return;
    const message = t(event.payload.code);
    showToast(message);
    playing = false; activeRoutes.clear();
    setPlayerStatus("error_prefix", { err: message });
    updatePlayerUi(); render();
  });
  void listen<{generation: number; code: string}>("airplay://session", (event) => {
    if (event.payload.generation < activeGeneration) return;
    activeGeneration = event.payload.generation;
    if (event.payload.code === "recovering") { playing = true; setPlayerStatus("session_recovering"); }
    if (event.payload.code === "streaming") {
      playing = true; setPlayerStatus("player_playing");
      if (!actionBusy && !latencyApplying) void syncNativeStream().catch(() => {});
    }
    if (event.payload.code === "auto_connecting") setPlayerStatus("auto_connecting");
    if (event.payload.code === "disconnected" && !actionBusy) { playing = false; activeRoutes.clear(); }
    updatePlayerUi();
  });
}

const known = new Map<string, Device>();
const discovered = new Map<string, Device>();
let unlisten: UnlistenFn | null = null;
let unlistenRemoved: UnlistenFn | null = null;
let connectedId: string | null = null;
let connectingId: string | null = null;
let connectedRoute: Device | null = null;
const activeRoutes = new Map<string, Device>();
let actionBusy = false;
let activeGeneration = 0;
let intentSequence = 0;
let stopping = false;
let nativeWindowVisible = true;
let lastDeviceRender = "";
let uiVolumeRevision = 0;
const uiVisible = () => nativeWindowVisible && !document.hidden;

const multiToggle = document.getElementById("multi-device") as HTMLInputElement;
const confirmDialog = document.getElementById("confirm-dialog") as HTMLDialogElement;
const confirmTitle = document.getElementById("confirm-title") as HTMLHeadingElement;
const confirmMessage = document.getElementById("confirm-message") as HTMLParagraphElement;
const confirmCancel = document.getElementById("confirm-cancel") as HTMLButtonElement;
const confirmAccept = document.getElementById("confirm-accept") as HTMLButtonElement;

function askConfirmation(title: string, message: string, acceptLabel: string): Promise<boolean> {
  confirmTitle.textContent = title;
  confirmMessage.textContent = message;
  confirmAccept.textContent = acceptLabel;
  return new Promise((resolve) => {
    confirmDialog.returnValue = "";
    confirmAccept.onclick = () => confirmDialog.close("accept");
    confirmCancel.onclick = () => confirmDialog.close("cancel");
    confirmDialog.addEventListener("close", () => resolve(confirmDialog.returnValue === "accept"), { once: true });
    confirmDialog.showModal();
  });
}

interface ConnectionInfo {
  ip: string;
  port: number;
  name: string;
}

async function streamTo(device: Device, command: "start_streaming" | "add_streaming", preferred?: Device | null): Promise<{ route: Device; ip: string }> {
  if (device.member_ids) {
    if (command === "add_streaming") throw new Error(t("stop_before_group_change"));
    if (!device.pair_complete) throw new Error(t("incomplete_pair"));
    await invoke("start_group_streaming", { deviceIds: device.member_ids, stereoPair: true, latencyMs: confirmedLatencyMs });
    activeGeneration = (await invoke<{generation: number}>("get_session_status")).generation;
    synchronizedPlaying = true;
    return { route: device, ip: device.addresses[0] };
  }
  const preferences = command === "add_streaming" || (playing && connectedId !== device.id)
    ? await invoke<ReceiverPreferences>("get_receiver_preferences", { receiverId: device.id }) : null;
  let lastError: unknown = new Error(t("no_device_ip"));
  for (const route of routesFor(discovered, device, preferred ?? null)) {
    const ip = route.addresses.find((a) => !a.includes(":")) ?? route.addresses[0];
    if (!ip) continue;
    try {
      await invoke(command, {
        deviceId: route.id,
        ip,
        port: route.port,
        name: device.name,
        volume: preferences?.volume ?? Number(volumeSlider.value) / 100,
        latencyMs: confirmedLatencyMs,
      });
      activeGeneration = (await invoke<{generation: number}>("get_session_status")).generation;
      if (command === "start_streaming") synchronizedPlaying = false;
      return { route, ip };
    } catch (err) {
      lastError = String(err) === "capture_interrupted" ? t("capture_interrupted") : err;
      if (!String(err).startsWith("stream:")) break;
    }
  }
  throw lastError;
}

async function connect(device: Device) {
  if (actionBusy || latencyApplying) return;
  if (playing && !multiToggle.checked) {
    const current = connectedId ? known.get(connectedId)?.name ?? t("player_playing") : t("player_playing");
    const confirmed = await askConfirmation(
      t("switch_title"),
      t("switch_message", { from: current, to: device.name }),
      t("switch_confirm", { name: device.name }),
    );
    if (!confirmed || actionBusy) return;
  }
  const intent = ++intentSequence;
  actionBusy = true;
  connectingId = device.id;
  showConnectionResult("connection_pairing");
  render();
  try {
    if (playing) {
      const { route, ip } = await streamTo(device, multiToggle.checked ? "add_streaming" : "start_streaming");
      if (intent !== intentSequence) return;
      if (!multiToggle.checked) {
        activeRoutes.clear();
        connectedId = device.id;
        connectedRoute = route;
        void invoke("save_last_device", { ip, port: route.port, name: device.name });
      }
      activeRoutes.set(device.id, route);
      return;
    }
    if (device.member_ids) { connectedId = device.id; connectedRoute = device; await loadReceiverPreferences(device.id); return; }
    let lastError: unknown = new Error("dispositivo sin dirección IP");
    let route: Device | null = null;
    for (const candidate of routesFor(discovered, device)) {
      const ip = candidate.addresses.find((a) => !a.includes(":")) ?? candidate.addresses[0];
      if (!ip) continue;
      try {
        await invoke<ConnectionInfo>("connect_device", {
          deviceId: candidate.id,
          ip,
          port: candidate.port,
          name: device.name,
        });
        route = candidate;
        break;
      } catch (err) {
        lastError = err;
        if (String(err) === "session_cancelled" || intent !== intentSequence) return;
      }
    }
    if (!route) throw lastError;
    if (intent !== intentSequence) return;
    connectedRoute = route;
    connectedId = device.id;
    await loadReceiverPreferences(device.id);
    showConnectionResult("connection_ready");
  } catch (err) {
    showConnectionResult("connection_failed", err);
  } finally {
    actionBusy = false;
    connectingId = null;
    render();
  }
}

async function disconnect(device: Device) {
  if (actionBusy || latencyApplying) return;
  actionBusy = true;
  render();
  try {
    if (playing && synchronizedPlaying) { await stopPlay(); return; }
    if (playing && activeRoutes.size > 1) {
      const route = activeRoutes.get(device.id);
      const ip = route?.addresses.find((a) => !a.includes(":")) ?? route?.addresses[0];
      if (!ip || !route) throw new Error(t("no_device_ip"));
      await invoke("remove_streaming", { ip, port: route.port });
      activeRoutes.delete(device.id);
      if (connectedId === device.id) {
        const next = activeRoutes.entries().next().value as [string, Device];
        connectedId = next[0];
        connectedRoute = next[1];
        const nextIp = next[1].addresses.find((a) => !a.includes(":")) ?? next[1].addresses[0];
        if (nextIp) void invoke("save_last_device", { ip: nextIp, port: next[1].port, name: known.get(next[0])?.name ?? next[1].name });
      }
      return;
    }
    if (playing) {
      await invoke("stop_streaming");
      playing = false;
      activeRoutes.clear();
      setPlayerStatus();
    }
    await invoke("disconnect_device");
    connectedId = null;
    connectedRoute = null;
  } catch (err) {
    statusEl.textContent = t("error_prefix", { err: String(err) });
  } finally {
    actionBusy = false;
    render();
  }
}

const playerEl = document.getElementById("player") as HTMLDivElement;
const playBtn = document.getElementById("play-stop") as HTMLButtonElement;
const playerStatus = document.getElementById("player-status") as HTMLSpanElement;
function setPlayerStatus(key = "", params?: Record<string, string | number>) {
  playerStatus.textContent = key ? t(key, params) : "";
  if (key && !params) playerStatus.dataset.i18n = key;
  else delete playerStatus.dataset.i18n;
}
const volumeSlider = document.getElementById("volume") as HTMLInputElement;
const volumeOut = document.getElementById("vol-out") as HTMLOutputElement;
type PlaybackMode = "ultra_low" | "balanced" | "stable";
const modes: PlaybackMode[] = ["ultra_low", "balanced", "stable"];
const modeButtons = modes.map(mode => document.getElementById(`mode-${mode}`) as HTMLButtonElement);
const modeDescription = document.getElementById("mode-description") as HTMLParagraphElement;
let confirmedMode: PlaybackMode = "balanced";
const diagnosticsExport = document.getElementById("diagnostics-export") as HTMLButtonElement;
const diagnosticsStatus = document.getElementById("diagnostics-status") as HTMLSpanElement;

diagnosticsExport.addEventListener("click", async () => {
  diagnosticsExport.disabled = true;
  try {
    const path = await invoke<string>("export_audio_diagnostics");
    diagnosticsStatus.textContent = `${t("diagnostics_saved")}\n${path}`;
    (document.getElementById("diagnostics-folder") as HTMLButtonElement).hidden = false;
  } catch (err) {
    diagnosticsStatus.textContent = t("diagnostics_error", { err: String(err) });
  } finally {
    diagnosticsExport.disabled = false;
  }
});

const latencyStatus = document.getElementById("latency-status") as HTMLSpanElement;
document.getElementById("diagnostics-folder")!.addEventListener("click", () => {
  void invoke("open_diagnostics_folder").catch(err => showToast(t("diagnostics_error", {err: String(err)})));
});

let playing = false;
let synchronizedPlaying = false;
let volumeDebounce: number | null = null;
const confirmedLatencyMs = 500; // receiver request, never displayed as measured delay
let latencyApplying = false;
let latencyCooldownUntil = 0;
let latencyTimer: number | null = null;

function updatePlayerUi() {
  const dev = connectedId ? known.get(connectedId) : null;
  playerEl.hidden = !dev;
  playBtn.disabled = stopping || ((latencyApplying || actionBusy) && !playing);
  cancelOperation.hidden = !actionBusy && !latencyApplying;
  audioSource.disabled = playing || actionBusy || latencyApplying;
  stereoPairs.disabled = playing || actionBusy || latencyApplying;
  groupPlay.disabled = playing || actionBusy || latencyApplying;
  groupFirst.disabled = groupSecond.disabled = playing || actionBusy || latencyApplying;
  refreshLatencyUi();
  if (!dev) return;
  if (playing) {
    playBtn.textContent = t("stop");
    playBtn.classList.add("playing");
  } else {
    playBtn.textContent = t("play_to", { name: dev.name });
    playBtn.classList.remove("playing");
  }
}

async function startPlay() {
  if (actionBusy || latencyApplying) return;
  const dev = connectedId ? known.get(connectedId) : null;
  if (!dev) return;
  const intent = ++intentSequence;
  actionBusy = true;
  playBtn.disabled = true;
  cancelOperation.hidden = false;
  setPlayerStatus("player_starting");
  try {
    const vol = Number(volumeSlider.value) / 100;
    const { route, ip } = await streamTo(dev, "start_streaming", connectedRoute);
    if (intent !== intentSequence) return;
    connectedRoute = route;
    activeRoutes.clear();
    activeRoutes.set(dev.id, route);
    playing = true;
    setPlayerStatus("player_playing");
    showConnectionResult("connection_streaming");
    void invoke("save_last_device", { ip, port: route.port, name: dev.name });
    // Persistimos para auto-reconnect (C2) y carga rápida en futuros arranques.
    void invoke("save_volume", { volume: vol });
  } catch (err) {
    setPlayerStatus("error_prefix", { err: String(err) });
  } finally {
    actionBusy = false;
    updatePlayerUi();
    render();
  }
}

async function stopPlay() {
  if (stopping) return;
  ++intentSequence;
  stopping = true;
  actionBusy = true;
  playBtn.disabled = true;
  try {
    await invoke("stop_streaming");
    await invoke("disconnect_device");
    playing = false; synchronizedPlaying = false;
    activeGeneration = (await invoke<{generation: number}>("get_session_status")).generation;
    activeRoutes.clear();
    setPlayerStatus();
    connectionResult.hidden = true;
  } catch (error) { showToast(t("error_prefix", { err: String(error) })); } finally {
    stopping = false;
    actionBusy = false;
    updatePlayerUi();
    render();
  }
}

playBtn.addEventListener("click", () => {
  if (playing) void stopPlay();
  else void startPlay();
});

multiToggle.addEventListener("change", async () => {
  const enabled = multiToggle.checked;
  if (actionBusy || latencyApplying) {
    multiToggle.checked = !enabled;
    return;
  }
  if (!enabled && playing && activeRoutes.size > 1) {
    const keepId = connectedId && activeRoutes.has(connectedId)
      ? connectedId
      : activeRoutes.keys().next().value as string;
    const keepName = known.get(keepId)?.name ?? t("player_playing");
    const confirmed = await askConfirmation(
      t("multi_off_title"),
      t("multi_off_message", { name: keepName }),
      t("multi_off_confirm", { name: keepName }),
    );
    if (!confirmed) {
      multiToggle.checked = true;
      return;
    }
    actionBusy = true;
    render();
    try {
      for (const [id, route] of [...activeRoutes]) {
        if (id === keepId) continue;
        const ip = route.addresses.find((a) => !a.includes(":")) ?? route.addresses[0];
        if (!ip) throw new Error("dispositivo sin dirección IP");
        await invoke("remove_streaming", { ip, port: route.port });
        activeRoutes.delete(id);
      }
    } catch (err) {
      multiToggle.checked = true;
      showToast(t("error_prefix", { err: String(err) }));
      return;
    } finally {
      actionBusy = false;
      render();
    }
  }
  try {
    await invoke("save_multi_device", { enabled });
  } catch (err) {
    multiToggle.checked = !enabled;
    showToast(t("error_prefix", { err: String(err) }));
  }
});

const volumeChanges = new LatestVolume(async vol => {
  await invoke("save_volume", {volume: vol});
  if (playing) await invoke("set_stream_volume", {volume: vol});
}, error => setPlayerStatus("vol_error_prefix", {err: String(error)}));

volumeSlider.addEventListener("input", () => {
  ++uiVolumeRevision;
  volumeOut.textContent = `${volumeSlider.value}%`;
  if (volumeDebounce) clearTimeout(volumeDebounce);
  volumeDebounce = window.setTimeout(() => {
    const vol = Number(volumeSlider.value) / 100;
    volumeChanges.set(vol);
  }, 120);
});

function refreshLatencyUi() {
  const cooldownMs = Math.max(0, latencyCooldownUntil - Date.now());
  modeButtons.forEach((button, index) => {
    button.disabled = actionBusy || latencyApplying || cooldownMs > 0;
    button.setAttribute("aria-pressed", String(modes[index] === confirmedMode));
  });
  modeDescription.textContent = t(`mode_${confirmedMode}_hint`);
  if (latencyApplying) latencyStatus.textContent = t("latency_applying");
  else if (cooldownMs > 0)
    latencyStatus.textContent = t("latency_cooldown", { seconds: Math.ceil(cooldownMs / 1000) });
  else latencyStatus.textContent = t("latency_current");
}

function setLatencyCooldown(ms: number) {
  latencyCooldownUntil = Date.now() + ms;
  if (latencyTimer !== null) clearInterval(latencyTimer);
  latencyTimer = window.setInterval(() => {
    refreshLatencyUi();
    if (Date.now() >= latencyCooldownUntil && latencyTimer !== null) {
      clearInterval(latencyTimer);
      latencyTimer = null;
    }
  }, 250);
  refreshLatencyUi();
}

modeButtons.forEach((button, index) => button.addEventListener("click", async () => {
  const mode = modes[index];
  if (actionBusy || latencyApplying || Date.now() < latencyCooldownUntil || mode === confirmedMode) return;
  if (mode === "ultra_low" && !await askConfirmation(t("mode_ultra_low"), t("mode_ultra_low_hint"), t("mode_enable"))) return;
  if (actionBusy || latencyApplying || stopping) return;
  latencyApplying = true;
  updatePlayerUi();
  refreshLatencyUi();
  try {
    const restarted = await invoke<boolean>("confirm_latency", { latencyMs: 500, playbackMode: mode });
    activeGeneration = (await invoke<{generation: number}>("get_session_status")).generation;
    confirmedMode = mode;
    if (restarted && !stopping) {
      playing = true;
      setPlayerStatus("player_playing");
    }
    setLatencyCooldown(10_000);
  } catch (err) {
    const message = String(err);
    const cooldown = /^latency_cooldown:(\d+)$/.exec(message);
    if (cooldown) setLatencyCooldown(Number(cooldown[1]));
    else showToast(t("latency_error", { err: message }));
    try {
      playing = (await invoke<ConnectionInfo | null>("is_streaming")) !== null;
      setPlayerStatus(playing ? "player_playing" : "");
    } catch {
      // Keep the previous UI state if the status query itself fails.
    }
  } finally {
    latencyApplying = false;
    refreshLatencyUi();
    updatePlayerUi();
    render();
  }
}));

function render() {
  if (!uiVisible()) return;
  const signature = JSON.stringify([getLang(), connectedId, connectingId, playing, synchronizedPlaying,
    actionBusy, latencyApplying, [...activeRoutes.keys()], [...known.values()]]);
  if (signature === lastDeviceRender) { updatePlayerUi(); return; }
  lastDeviceRender = signature;
  devicesList.replaceChildren();
  const sorted = [...known.values()].sort((a, b) => {
    if (a.kind === "homepod" && b.kind !== "homepod") return -1;
    if (b.kind === "homepod" && a.kind !== "homepod") return 1;
    return a.name.localeCompare(b.name);
  });
  for (const d of sorted) {
    const li = document.createElement("li");
    const isConnected = playing ? activeRoutes.has(d.id) : connectedId === d.id;
    const isConnecting = connectingId === d.id;
    li.className = `device ${d.kind}${isConnected ? " connected" : ""}`;
    const addr = d.addresses.find((a) => !a.includes(":")) ?? d.addresses[0] ?? d.host;

    const info = document.createElement("div");
    info.className = "info";
    info.innerHTML = `
      <span class="name">${escape(d.name)}</span>
      <span class="meta">${d.member_ids ? `${escape(t("stereo_pair"))} · ${escape(d.member_names?.join(" + ") ?? "")}` : `${KIND_LABEL[d.kind]} · ${escape(addr)}:${d.port}${d.supports_airplay2 ? " · AirPlay 2" : ""}`}</span>
    `;
    li.appendChild(info);

    const btn = document.createElement("button");
    btn.className = "connect";
    if (isConnecting) {
      btn.textContent = t("connecting");
      btn.disabled = true;
    } else if (isConnected) {
      btn.textContent = t("disconnect");
      btn.disabled = actionBusy || latencyApplying;
      btn.addEventListener("click", () => void disconnect(d));
    } else {
      btn.textContent = t("connect");
      btn.disabled = actionBusy || latencyApplying || d.available === false || (Boolean(d.member_ids) && !d.pair_complete);
      if (d.member_ids && !d.pair_complete) btn.textContent = t("incomplete_pair");
      btn.addEventListener("click", () => void connect(d));
    }
    li.appendChild(btn);

    devicesList.appendChild(li);
  }
  statusEl.textContent =
    known.size === 1
      ? t("devices_count_one")
      : t("devices_count_other", { n: known.size });
  updatePlayerUi();
  multiToggle.disabled = synchronizedPlaying || actionBusy || latencyApplying;
  refreshLatencyUi();
}

function refreshKnownDevices() {
  const previous = [...known.values()];
  known.clear();
  for (const [key, device] of groupDevices(discovered, stereoPairs.checked)) known.set(key, device);
  for (const device of previous) { if (!known.has(device.id) && (connectedId === device.id || activeRoutes.has(device.id))) known.set(device.id, {...device, available: false}); }
  refreshGroupChoices();
}

function escape(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!,
  );
}

async function startScan() {
  scanBtn.disabled = true;
  statusEl.textContent = t("scan_searching");
  discovered.clear();
  refreshKnownDevices();
  render();

  if (unlisten) {
    unlisten();
    unlisten = null;
  }

  unlistenRemoved?.();
  unlistenRemoved = await listen<string>("airplay://device-removed", event => { discovered.delete(event.payload); refreshKnownDevices(); render(); });
  unlisten = await listen<Device>("airplay://device", (event) => {
    discovered.set(event.payload.id, event.payload);
    refreshKnownDevices();
    render();
  });

  try {
    await invoke("start_discovery_stream");
  } catch (err) {
    statusEl.textContent = t("error_prefix", { err: String(err) });
  }

  scanBtn.disabled = false;
}

scanBtn.addEventListener("click", () => {
  void startScan();
});

const manualIpInput = document.getElementById("manual-ip") as HTMLInputElement;
const manualNameInput = document.getElementById("manual-name") as HTMLInputElement;
const manualBtn = document.getElementById("manual-add") as HTMLButtonElement;
const manualStatus = document.getElementById("manual-status") as HTMLSpanElement;

async function addManual() {
  const ip = manualIpInput.value.trim();
  const name = manualNameInput.value.trim() || null;
  if (!ip) {
    manualStatus.textContent = t("manual_need_ip");
    return;
  }
  manualBtn.disabled = true;
  manualStatus.textContent = t("manual_checking");
  try {
    const device = await invoke<Device>("add_manual_device", {
      ip,
      port: null,
      name,
    });
    manualStatus.textContent = t("manual_ok", { name: device.name });
    manualIpInput.value = "";
    manualNameInput.value = "";
  } catch (err) {
    manualStatus.textContent = String(err);
  } finally {
    manualBtn.disabled = false;
  }
}

manualBtn.addEventListener("click", () => {
  void addManual();
});
manualIpInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") void addManual();
});

setupAsyncErrorListener();
setupLangToggle();
// Initialize after all controls below have been created.
queueMicrotask(() => void initialize());

async function initialize() {
  try { setLang(await invoke<Lang>("get_ui_language")); }
  catch { setLang("zh"); }
  try { setUiVisibility(await invoke<boolean>("get_main_window_visible")); } catch { setUiVisibility(true); }
  await Promise.all([preloadSavedVolume(), preloadSavedLatency(), preloadMultiDevice(), loadDailySettings(), loadAudioOptions().catch(e => showToast(String(e)))]);
  await bootstrap();
}

async function preloadMultiDevice() {
  try {
    multiToggle.checked = await invoke<boolean>("get_multi_device");
  } catch {
    multiToggle.checked = false;
  }
}

function setupLangToggle() {
  const btn = document.getElementById("lang-toggle") as HTMLButtonElement | null;
  applyStaticTranslations();
  refreshLangToggleLabel();
  syncTrayLanguage(getLang());
  onLangChange(() => {
    refreshLangToggleLabel();
    syncTrayLanguage(getLang());
    // Re-render lo que tiene texto dinámico generado por JS.
    render();
    updatePlayerUi();
    refreshLatencyUi();
    void refreshAudioOutputs().catch(e => showToast(String(e)));
  });
  const selector = document.getElementById("language-select") as HTMLSelectElement;
  selector.value = getLang();
  if (btn) btn.addEventListener("click", () => { selector.hidden = !selector.hidden; if (!selector.hidden) selector.focus(); });
  selector.addEventListener("change", async () => {
    const lang = selector.value as Lang;
    try {
      await invoke("save_ui_language", { lang });
      setLang(lang);
      selector.hidden = true;
    } catch {
      selector.value = getLang();
      showToast(t("language_save_error"));
    }
  });
}

function syncTrayLanguage(lang: Lang) {
  void invoke("set_tray_language", { lang }).catch((err) => {
    console.error("Could not update tray language", err);
  });
}

function refreshLangToggleLabel() {
  const btn = document.getElementById("lang-toggle") as HTMLButtonElement | null;
  if (!btn) return;
  btn.textContent = t("language");
  const title = t("lang_toggle_title");
  btn.title = title;
  btn.setAttribute("aria-label", title);
  (document.getElementById("language-select") as HTMLSelectElement).value = getLang();
}

async function preloadSavedVolume() {
  try {
    const v = await invoke<number | null>("get_volume");
    if (v !== null && v !== undefined) {
      const pct = Math.round(Math.max(0, Math.min(1, v)) * 100);
      volumeSlider.value = String(pct);
      volumeOut.textContent = `${pct}%`;
    }
  } catch {
    // sin volumen guardado todavía, se queda el default del HTML.
  }
}

async function preloadSavedLatency() {
  try {
    const [mode, cooldownMs] = await Promise.all([
      invoke<PlaybackMode>("get_playback_mode"),
      invoke<number>("get_latency_cooldown_ms"),
    ]);
    if (modes.includes(mode)) confirmedMode = mode;
    if (cooldownMs > 0) setLatencyCooldown(cooldownMs);
  } catch {
    // Keep the safe default when no setting has been stored yet.
  }
  refreshLatencyUi();
}


async function bootstrap() {
  void startScan();
  await syncNativeStream();
}

async function syncNativeStream() {
  if (actionBusy || latencyApplying || stopping) return;
  const revision = uiVolumeRevision, intent = intentSequence, generation = activeGeneration;
  const streams = await invoke<{ip: string; port: number; name: string; volume: number}[]>("list_streaming");
  if (!streams.length || actionBusy || latencyApplying || stopping || intent !== intentSequence || generation !== activeGeneration) return;
  const stream = streams[0];
  const dev = [...discovered.values()].find(d => d.port === stream.port && d.addresses.includes(stream.ip))
    ?? { id: `manual://${stream.ip}:${stream.port}`, name: stream.name, addresses: [stream.ip], port: stream.port,
      kind: "homepod", available: true, manual: true, supports_airplay2: true } as Device;
  discovered.set(dev.id, dev); refreshKnownDevices();
  const display = [...known.values()].find(d => d.addresses.includes(stream.ip)) ?? dev;
  connectedId = display.id; connectedRoute = dev; activeRoutes.set(display.id, dev);
  playing = true;
  if (revision === uiVolumeRevision) { volumeSlider.value = String(Math.round(stream.volume * 100)); volumeOut.value = `${volumeSlider.value}%`; }
  setPlayerStatus("player_playing"); updatePlayerUi(); render();
}

interface ReceiverPreferences { volume: number; latency_ms: number; reconnect: boolean }
async function patchReceiverPreferences(id: string, patch: Partial<ReceiverPreferences>) {
  await invoke("patch_receiver_preferences", { receiverId: id, patch });
}
async function loadReceiverPreferences(id: string) {
  const preferences = await invoke<ReceiverPreferences>("get_receiver_preferences", { receiverId: id });
  if (connectedId !== id) return;
  // The one global volume is authoritative; ignore obsolete duplicate slider values.
  refreshLatencyUi();
}
const cancelOperation = document.getElementById("operation-cancel") as HTMLButtonElement;
cancelOperation.addEventListener("click", () => void stopPlay());
const audioSource = document.getElementById("audio-source") as HTMLSelectElement;
const audioStatus = document.getElementById("audio-options-status") as HTMLSpanElement;
const launchAtLogin = document.getElementById("launch-at-login") as HTMLInputElement;
const automaticConnect = document.getElementById("auto-connect") as HTMLInputElement;
const dailyStatus = document.getElementById("daily-status") as HTMLSpanElement;
async function loadDailySettings() {
  try {
    const settings = await invoke<{launch_at_login: boolean; startup_supported: boolean; auto_connect: boolean}>("get_daily_settings");
    launchAtLogin.checked = settings.launch_at_login;
    launchAtLogin.disabled = !settings.startup_supported;
    automaticConnect.checked = settings.auto_connect;
    if (!settings.startup_supported) dailyStatus.textContent = t("startup_packaged_pending");
  } catch (err) { dailyStatus.textContent = t("settings_error", {err: String(err)}); }
}
launchAtLogin.addEventListener("change", async () => {
  const enabled = launchAtLogin.checked;
  launchAtLogin.disabled = true;
  try {
    if (enabled && !await askConfirmation(t("launch_at_login"), t("startup_consent"), t("enable_setting"))) {
      launchAtLogin.checked = false; return;
    }
    await invoke("set_launch_at_login", {enabled});
    dailyStatus.textContent = t("settings_saved");
  } catch (err) { launchAtLogin.checked = !enabled; dailyStatus.textContent = t("settings_error", {err: String(err)}); }
  finally { launchAtLogin.disabled = false; }
});
automaticConnect.addEventListener("change", async () => {
  const enabled = automaticConnect.checked;
  automaticConnect.disabled = true;
  try {
    if (enabled && !await askConfirmation(t("auto_connect"), t("auto_connect_hint"), t("enable_setting"))) {
      automaticConnect.checked = false; return;
    }
    await invoke("set_auto_connect", {enabled});
    dailyStatus.textContent = t("settings_saved");
  } catch (err) { automaticConnect.checked = !enabled; dailyStatus.textContent = t("settings_error", {err: String(err)}); }
  finally { automaticConnect.disabled = false; }
});
interface AudioOptions { device_id: string | null; mute_local: boolean }
let savedAudioOptions: AudioOptions = { device_id: null, mute_local: false };
async function loadAudioOptions() {
  savedAudioOptions = await invoke<AudioOptions>("get_audio_options");
  await refreshAudioOutputs();
}
async function refreshAudioOutputs() {
  const devices = await invoke<{id: string; name: string; is_default: boolean}[]>("list_audio_outputs");
  audioSource.replaceChildren(new Option(t("audio_default"), ""));
  for (const device of devices) audioSource.add(new Option(device.name + (device.is_default ? ` (${t("audio_default")})` : ""), device.id));
  if (savedAudioOptions.device_id && !devices.some(d => d.id === savedAudioOptions.device_id)) audioSource.add(new Option(t("receiver_unavailable"), savedAudioOptions.device_id));
  audioSource.value = savedAudioOptions.device_id ?? "";
  updatePlayerUi();
}
async function saveAudioOptions() {
  try {
    const options = { device_id: audioSource.value || null, mute_local: false };
    await invoke("save_audio_options", { options }); savedAudioOptions = options; audioStatus.textContent = "";
  } catch (error) {
    audioSource.value = savedAudioOptions.device_id ?? "";
    audioStatus.textContent = t(String(error));
  }
}
audioSource.addEventListener("change", () => void saveAudioOptions());
document.getElementById("audio-refresh")!.addEventListener("click", () => void refreshAudioOutputs().catch(e => showToast(String(e))));
const stereoPairs = document.getElementById("stereo-pairs") as HTMLInputElement;
const groupFirst = document.getElementById("group-first") as HTMLSelectElement;
const groupSecond = document.getElementById("group-second") as HTMLSelectElement;
const groupPlay = document.getElementById("group-play") as HTMLButtonElement;
const groupStatus = document.getElementById("group-status") as HTMLSpanElement;
stereoPairs.addEventListener("change", () => { if (playing) { stereoPairs.checked = !stereoPairs.checked; return; } refreshKnownDevices(); render(); });
function refreshGroupChoices() {
  const first = groupFirst.value, second = groupSecond.value;
  groupFirst.replaceChildren(); groupSecond.replaceChildren();
  for (const device of groupDevices(discovered).values()) {
    if (device.kind !== "homepod") continue;
    const route = routesFor(discovered, device)[0];
    groupFirst.add(new Option(device.name, route.id)); groupSecond.add(new Option(device.name, route.id));
  }
  if (Array.from(groupFirst.options).some(o => o.value === first)) groupFirst.value = first;
  if (second !== groupFirst.value && Array.from(groupSecond.options).some(o => o.value === second)) groupSecond.value = second;
  else if (groupSecond.options.length > 1) groupSecond.selectedIndex = Array.from(groupSecond.options).findIndex(o => o.value !== groupFirst.value);
}
groupPlay.addEventListener("click", async () => {
  if (playing || actionBusy || latencyApplying) return;
  if (!groupFirst.value || groupFirst.value === groupSecond.value) { groupStatus.textContent = t("group_requires_two_receivers"); return; }
  const selected = [groupFirst.value, groupSecond.value].map(id => discovered.get(id)!);
  actionBusy = true; const intent = ++intentSequence; cancelOperation.hidden = false; render();
  try {
    await invoke("start_group_streaming", { deviceIds: selected.map(d => d.id), stereoPair: false, latencyMs: confirmedLatencyMs });
    if (intent !== intentSequence) return;
    activeGeneration = (await invoke<{generation: number}>("get_session_status")).generation;
    activeRoutes.clear();
    for (const route of selected) { activeRoutes.set([...groupDevices(discovered)].find(([,d]) => d.hardware_id === route.hardware_id)?.[0] ?? route.id, route); }
    connectedId = activeRoutes.keys().next().value ?? null;
    connectedRoute = connectedId ? activeRoutes.get(connectedId)! : null;
    playing = true; synchronizedPlaying = true; multiToggle.checked = false;
    if (connectedId) await loadReceiverPreferences(connectedId);
    setPlayerStatus("player_playing"); groupStatus.textContent = "";
  } catch (error) { groupStatus.textContent = t("error_prefix", { err: String(error) }); }
  finally { actionBusy = false; updatePlayerUi(); render(); }
});
const uiPoller = new VisiblePoller(async () => {
    if (actionBusy || latencyApplying || stopping) return null;
    const status = await invoke<{generation: number; status: string; mute_status: string; group_mode?: boolean}>("get_session_status");
    if (status.status === "busy") return null;
    return { status };
  }, result => {
    if (!result || !uiVisible() || actionBusy || latencyApplying || stopping) return;
    const {status} = result;
    if (status.generation < activeGeneration) return;
    activeGeneration = status.generation;
    synchronizedPlaying = status.group_mode ?? synchronizedPlaying;
    if (status.status === "streaming" && (!playing || !connectedId)) void syncNativeStream().catch(() => {});
    if (status.status === "disconnected") { playing = false; activeRoutes.clear(); updatePlayerUi(); render(); }
  });

function setUiVisibility(visible: boolean) {
  nativeWindowVisible = visible;
  uiPoller.setVisible(uiVisible());
  if (uiVisible()) render();
}
void listen<boolean>("app://ui-visible", event => setUiVisibility(event.payload));
document.addEventListener("visibilitychange", () => setUiVisibility(nativeWindowVisible));
window.addEventListener("pagehide", () => uiPoller.dispose(), { once: true });

// Fixed local legal documents; always textContent, never remote HTML.
void listen<number>("app://volume-changed",event=>{
  ++uiVolumeRevision;volumeSlider.value=String(Math.round(event.payload*100));volumeOut.textContent=`${volumeSlider.value}%`;
});
const legalDialog = document.getElementById("legal-dialog") as HTMLDialogElement;
document.getElementById("legal-close")!.addEventListener("click", () => legalDialog.close());
document.querySelectorAll<HTMLButtonElement>("[data-legal]").forEach(button => {
  button.addEventListener("click", () => {
    void invoke<string>("get_legal_document", { kind: button.dataset.legal }).then(text => {
      document.getElementById("legal-title")!.textContent = button.textContent;
      document.getElementById("legal-content")!.textContent = text;
      legalDialog.showModal();
    }).catch(error => showToast(t(String(error))));
  });
});
void invoke<{version:string;revision:string|null;source_fingerprint:string|null}>("get_build_info").then(info => {
  document.getElementById("build-info")!.textContent =
    `DeskAudio Cast ${info.version} · ${info.source_fingerprint?.slice(0,12) ?? "development"}`;
});
import {confirmedOperation} from "./confirmed-operation.mjs";
document.getElementById("clear-device-data")!.addEventListener("click", () => {
  void (async () => {
    const status = document.getElementById("privacy-status")!;
    if (playing || actionBusy || connectingId) { status.textContent = t("stop_before_clear"); return; }
    try {
      if (!await confirmedOperation(
        () => askConfirmation(t("clear_device_data"), t("privacy_clear_hint"), t("clear_device_data")),
        () => invoke<void>("clear_saved_device_data"))) return;
      connectedId = null; connectedRoute = null;
      status.textContent = t("device_data_cleared");
      (document.getElementById("auto-connect") as HTMLInputElement).checked = false;
      audioSource.value = "";
      savedAudioOptions = { device_id: null, mute_local: false };
      render();
    } catch (error) { status.textContent = t(String(error)); }
  })();
});
