/**
 * The Tauri command surface, wrapped once so screens never touch `invoke`
 * directly and so the app degrades gracefully when it runs in a plain browser
 * during development.
 */

import { invoke } from "@tauri-apps/api/core";

import type {
  AppConfig,
  AppInfo,
  BackendOption,
  CaptureBackendKind,
  DiscoveredDevice,
  DisplayInfo,
  EngineStatus,
  Layout,
  ScriptExample,
  WizardParams,
} from "./types";

/** True when running inside the Tauri shell rather than a browser tab. */
export const isDesktop =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isDesktop) {
    // Browser preview: serve the demo backend so every screen is reachable
    // without the desktop shell or any hardware.
    const { mockBackend } = await import("./mock");
    const handler = mockBackend[command];
    if (!handler) throw new Error(`command ${command} needs the desktop app`);
    return handler(args as never) as Promise<T>;
  }
  return invoke<T>(command, args);
}

export const api = {
  getConfig: () => call<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => call<AppConfig>("save_config", { config }),
  getStatus: () => call<EngineStatus>("get_status"),
  setEnabled: (enabled: boolean) => call<void>("set_enabled", { enabled }),
  setActiveProfile: (id: string) => call<AppConfig>("set_active_profile", { id }),
  deleteProfile: (id: string) => call<AppConfig>("delete_profile", { id }),
  listDisplays: (backend?: CaptureBackendKind) =>
    call<DisplayInfo[]>("list_displays", { backend: backend ?? null }),
  listCaptureBackends: () => call<BackendOption[]>("list_capture_backends"),
  listAudioDevices: () => call<string[]>("list_audio_devices"),
  listScriptExamples: () => call<ScriptExample[]>("list_script_examples"),
  discoverDevices: (timeoutMs = 2500) =>
    call<DiscoveredDevice[]>("discover_devices", { timeoutMs }),
  probeDevice: (host: string) =>
    call<DiscoveredDevice | null>("probe_device", { host }),
  buildLayout: (params: WizardParams) => call<Layout>("build_layout", { params }),
  identifyLed: (index: number, ms = 1200) =>
    call<void>("identify_led", { index, ms }),
  holdColor: (color: string | null) => call<void>("hold_color", { color }),
  regenerateApiToken: () => call<string>("regenerate_api_token"),
  setLaunchAtLogin: (enabled: boolean) =>
    call<void>("set_launch_at_login", { enabled }),
  appInfo: () => call<AppInfo>("app_info"),
  openConfigDir: () => call<void>("open_config_dir"),
};

/** Format a Rust error, which arrives as a plain string. */
export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
