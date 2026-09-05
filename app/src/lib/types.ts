/**
 * Mirrors of the Rust types that cross the Tauri boundary.
 *
 * Keep these in step with `maslight-core` and `maslight-engine`. Field names
 * are camelCase because every serialised struct carries
 * `#[serde(rename_all = "camelCase")]`.
 */

export type ColorOrder = "rgb" | "rbg" | "grb" | "gbr" | "brg" | "bgr";
export type LuminancePolicy = "minimum-level" | "dead-zone";
export type RgbwMode = "none" | "subtract" | "additive";
export type LightMode = "screen" | "audio" | "effect" | "off";
export type Corner = "bottom-left" | "bottom-right" | "top-left" | "top-right";
export type Direction = "clockwise" | "counter-clockwise";
export type WledProtocol = "warls" | "drgb" | "dnrgb" | "drgbw" | "dnrgbw";
export type SerialProtocol = "adalight" | "tpm2";
export type CaptureBackendKind =
  | "auto"
  | "dxgi"
  | "x11"
  | "pipe-wire"
  | "screen-capture-kit"
  | "test";

export interface Rgb8 {
  r: number;
  g: number;
  b: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface DisplayRegion {
  id: string;
  label: string;
  bounds: [number, number, number, number];
}

export interface LedSpec {
  index: number;
  display: string;
  rect: Rect;
  weight: number;
  enabled: boolean;
}

export interface ChainConfig {
  colorOrder: ColorOrder;
  rgbw: boolean;
  reverse: boolean;
}

export interface Layout {
  version: number;
  name: string;
  displays: DisplayRegion[];
  chain: ChainConfig;
  leds: LedSpec[];
}

export interface EdgeCounts {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface WizardParams {
  name: string;
  display: string;
  counts: EdgeCounts;
  startCorner: Corner;
  direction: Direction;
  depth: number;
  span: number;
  chainOffset: number;
  reverseChain: boolean;
  chain: ChainConfig;
}

export interface ColorSettings {
  brightness: number;
  gamma: number;
  saturation: number;
  vibrance: number;
  temperatureK: number;
  channelGain: [number, number, number];
  luminanceThreshold: number;
  luminancePolicy: LuminancePolicy;
  smoothingMs: number;
  maxSlewPerS: number | null;
  dithering: boolean;
  hdrToneMap: boolean;
  hdrPeak: number;
  powerLimitA: number | null;
  ledChannelMa: number;
  rgbwMode: RgbwMode;
}

export interface CaptureSettings {
  backend: CaptureBackendKind;
  targetFps: number;
  adaptive: boolean;
  idleFps: number;
  idleThreshold: number;
  hdr: boolean;
  detectBlackBars: boolean;
  sampleGrid: number;
}

export type DeviceConfig =
  | {
      kind: "wled";
      host: string;
      port: number;
      protocol: WledProtocol;
      timeoutS: number;
    }
  | { kind: "ddp"; host: string; port: number; startOffset: number }
  | { kind: "e131"; host: string | null; universe: number; priority: number }
  | {
      kind: "art-net";
      host: string;
      port: number;
      universe: number;
      net: number;
      subnet: number;
    }
  | { kind: "serial"; port: string; baud: number; protocol: SerialProtocol }
  | { kind: "null" };

export interface Profile {
  id: string;
  name: string;
  mode: LightMode;
  layout: Layout;
  color: ColorSettings;
  capture: CaptureSettings;
  devices: DeviceConfig[];
  latencyMs: number;
  audioBlend: number;
}

export type AutoRule =
  | { when: "process-running"; process: string; profile: string }
  | { when: "fullscreen"; profile: string }
  | {
      when: "time-range";
      fromMinutes: number;
      toMinutes: number;
      profile: string;
    }
  | { when: "on-battery"; profile: string };

export interface UiPrefs {
  language: string;
  theme: string;
  startMinimised: boolean;
  launchAtLogin: boolean;
  checkForUpdates: boolean;
  enableApi: boolean;
  apiPort: number;
}

export interface AppConfig {
  version: number;
  activeProfile: string;
  profiles: Profile[];
  rules: AutoRule[];
  ui: UiPrefs;
  enabled: boolean;
}

export interface DeviceStatus {
  label: string;
  /** The transport is open. For UDP this is not proof anything is listening. */
  connected: boolean;
  /** Whether the controller answered when last asked. Null when not applicable. */
  reachable: boolean | null;
  error: string | null;
}

export interface EngineStatus {
  running: boolean;
  enabled: boolean;
  profile: string;
  profileId: string;
  mode: LightMode;
  fps: number;
  frameMs: number;
  idle: boolean;
  ledCount: number;
  leds: Rgb8[];
  devices: DeviceStatus[];
  captureBackend: string;
  display: string;
  sourceWidth: number;
  sourceHeight: number;
  insets: [number, number, number, number];
  frames: number;
  lastError: string | null;
}

export interface DisplayInfo {
  id: string;
  label: string;
  width: number;
  height: number;
  x: number;
  y: number;
  primary: boolean;
}

export interface DiscoveredDevice {
  name: string;
  host: string;
  port: number;
  ledCount: number | null;
  version: string | null;
  kind: string;
}

export interface BackendOption {
  kind: CaptureBackendKind;
  label: string;
}

export interface AppInfo {
  version: string;
  platform: string;
  configPath: string;
  autostartSupported: boolean;
  serialSupported: boolean;
}

/** Find the profile the configuration points at. */
export function activeProfile(config: AppConfig): Profile {
  return (
    config.profiles.find((p) => p.id === config.activeProfile) ??
    config.profiles[0]
  );
}

/** Replace the active profile, returning a new configuration. */
export function withActiveProfile(
  config: AppConfig,
  update: (profile: Profile) => Profile
): AppConfig {
  const active = activeProfile(config);
  return {
    ...config,
    profiles: config.profiles.map((p) => (p.id === active.id ? update(p) : p)),
  };
}

export function deviceLabel(device: DeviceConfig): string {
  switch (device.kind) {
    case "wled":
      return `WLED ${device.host}`;
    case "ddp":
      return `DDP ${device.host}`;
    case "e131":
      return device.host
        ? `sACN ${device.host} u${device.universe}`
        : `sACN multicast u${device.universe}`;
    case "art-net":
      return `Art-Net ${device.host} u${device.universe}`;
    case "serial":
      return `Serial ${device.port}`;
    default:
      return "Disconnected";
  }
}

export function rgbToCss(c: Rgb8): string {
  return `rgb(${c.r}, ${c.g}, ${c.b})`;
}
