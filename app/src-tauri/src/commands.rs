//! Commands the interface calls.
//!
//! Every command is small and total: it either returns data or a string the UI
//! can show. Nothing here blocks for long, because these run on the Tauri
//! thread pool and the engine keeps running regardless.

use maslight_capture::DisplayInfo;
use maslight_core::{AppConfig, CaptureBackendKind, Layout, WizardParams};
use maslight_engine::EngineStatus;
use maslight_output::discovery::DiscoveredDevice;
use serde::Serialize;
use tauri::State;

use crate::{parse_hex, AppState};

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config()
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    state.commit(config)?;
    Ok(state.config())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> EngineStatus {
    state.status()
}

#[tauri::command]
pub fn set_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let mut config = state.config();
    config.enabled = enabled;
    state.commit(config)
}

#[tauri::command]
pub fn set_active_profile(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let mut config = state.config();
    if config.profile(&id).is_none() {
        return Err(format!("no profile with id {id}"));
    }
    config.active_profile = id;
    state.commit(config)?;
    Ok(state.config())
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let mut config = state.config();
    if !config.remove(&id) {
        return Err(String::from("the last profile cannot be deleted"));
    }
    state.commit(config)?;
    Ok(state.config())
}

#[tauri::command]
pub fn list_displays(backend: Option<CaptureBackendKind>) -> Vec<DisplayInfo> {
    maslight_engine::list_displays(backend.unwrap_or_default())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendOption {
    pub kind: CaptureBackendKind,
    pub label: String,
}

#[tauri::command]
pub fn list_capture_backends() -> Vec<BackendOption> {
    maslight_capture::available_kinds()
        .into_iter()
        .map(|kind| BackendOption {
            kind,
            label: match kind {
                CaptureBackendKind::Auto => String::from("Automatic"),
                CaptureBackendKind::Dxgi => String::from("Windows Desktop Duplication"),
                CaptureBackendKind::X11 => String::from("X11"),
                CaptureBackendKind::PipeWire => String::from("PipeWire (Wayland)"),
                CaptureBackendKind::ScreenCaptureKit => String::from("ScreenCaptureKit"),
                CaptureBackendKind::Test => String::from("Test pattern"),
            },
        })
        .collect()
}

/// Audio devices the audio mode can listen to.
#[tauri::command]
pub fn list_audio_devices() -> Vec<String> {
    maslight_engine::list_audio_devices()
}

#[tauri::command]
pub async fn discover_devices(timeout_ms: Option<u64>) -> Vec<DiscoveredDevice> {
    let ms = timeout_ms.unwrap_or(2500).clamp(300, 15_000);
    // mDNS blocks, so keep it off the command thread pool.
    tauri::async_runtime::spawn_blocking(move || maslight_engine::discover_devices(ms))
        .await
        .unwrap_or_default()
}

/// Ask a controller at a known address about itself.
#[tauri::command]
pub async fn probe_device(host: String) -> Option<DiscoveredDevice> {
    tauri::async_runtime::spawn_blocking(move || {
        let info = maslight_output::discovery::query_wled_info(
            &host,
            std::time::Duration::from_millis(1200),
        )?;
        Some(DiscoveredDevice {
            name: info.name.clone().unwrap_or_else(|| host.clone()),
            host,
            port: 21324,
            led_count: info.led_count,
            version: info.version,
            kind: String::from("wled"),
        })
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub fn build_layout(params: WizardParams) -> Layout {
    Layout::from_wizard(&params)
}

#[tauri::command]
pub fn identify_led(state: State<'_, AppState>, index: usize, ms: Option<u64>) {
    state
        .shared
        .engine
        .lock()
        .identify(index, ms.unwrap_or(1200));
}

/// Drive the whole strip with one colour, or clear the hold with `None`.
#[tauri::command]
pub fn hold_color(state: State<'_, AppState>, color: Option<String>) {
    let parsed = color.as_deref().and_then(parse_hex);
    state.shared.engine.lock().hold_color(parsed);
}

#[tauri::command]
pub fn set_launch_at_login(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    crate::autostart::set(enabled)?;
    let mut config = state.config();
    config.ui.launch_at_login = enabled;
    state.commit(config)
}

/// Issue a new API token, revoking every script that had the old one.
#[tauri::command]
pub fn regenerate_api_token(state: State<'_, AppState>) -> Result<String, String> {
    let mut config = state.config();
    let token = maslight_api::generate_token();
    config.ui.api_token = token.clone();
    state.commit(config)?;
    Ok(token)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub config_path: String,
    pub autostart_supported: bool,
    pub serial_supported: bool,
    /// Port the local API is actually listening on, when it is running.
    pub api_port: Option<u16>,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        api_port: state.api.lock().as_ref().map(|s| s.port()),
        version: String::from(env!("CARGO_PKG_VERSION")),
        platform: String::from(std::env::consts::OS),
        config_path: maslight_core::profile::config_path()
            .to_string_lossy()
            .to_string(),
        autostart_supported: crate::autostart::supported(),
        serial_supported: cfg!(feature = "serial"),
    }
}

/// Reveal the configuration folder in the system file manager.
#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    let dir = maslight_core::profile::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(&dir).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&dir).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(&dir).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}
