/**
 * A stand-in backend for running the interface in a plain browser.
 *
 * `npm run dev` without the Tauri shell is the fastest way to work on the UI,
 * and a contributor with no LED hardware can still see every screen. The data
 * is deliberately obvious: a synthetic profile and a moving colour wheel.
 */

import type {
  AppConfig,
  AppInfo,
  BackendOption,
  DiscoveredDevice,
  DisplayInfo,
  EngineStatus,
  Layout,
  LedSpec,
  Rgb8,
  WizardParams,
} from "./types";

function edgeLayout(): Layout {
  const leds: LedSpec[] = [];
  const push = (x: number, y: number, w: number, h: number) =>
    leds.push({
      index: leds.length,
      display: "mock-1",
      rect: { x, y, w, h },
      weight: 1,
      enabled: true,
    });

  const depth = 0.12;
  const across = 20;
  const down = 12;
  for (let i = 0; i < across; i++)
    push(i / across, 1 - depth, 1 / across, depth);
  for (let i = 0; i < down; i++)
    push(1 - depth, 1 - (i + 1) / down, depth, 1 / down);
  for (let i = 0; i < across; i++)
    push(1 - (i + 1) / across, 0, 1 / across, depth);
  for (let i = 0; i < down; i++) push(0, i / down, depth, 1 / down);

  return {
    version: 1,
    name: "Demo",
    displays: [{ id: "mock-1", label: "Demo display", bounds: [0, 0, 2560, 1440] }],
    chain: { colorOrder: "grb", rgbw: false, reverse: false },
    leds,
  };
}

const config: AppConfig = {
  version: 1,
  activeProfile: "demo",
  enabled: true,
  rules: [],
  ui: {
    language: "system",
    theme: "dark",
    startMinimised: false,
    launchAtLogin: false,
    checkForUpdates: false,
    enableApi: false,
    apiPort: 4599,
  },
  profiles: [
    {
      id: "demo",
      name: "Desk",
      mode: "screen",
      layout: edgeLayout(),
      latencyMs: 0,
      audioBlend: 0,
      devices: [
        {
          kind: "wled",
          host: "192.168.0.200",
          port: 21324,
          protocol: "dnrgb",
          timeoutS: 2,
        },
      ],
      color: {
        brightness: 0.85,
        gamma: 1,
        saturation: 1.1,
        vibrance: 0.15,
        temperatureK: 6500,
        channelGain: [1, 1, 1],
        luminanceThreshold: 0.02,
        luminancePolicy: "minimum-level",
        smoothingMs: 90,
        maxSlewPerS: null,
        dithering: true,
        hdrToneMap: false,
        hdrPeak: 4,
        powerLimitA: null,
        ledChannelMa: 20,
        rgbwMode: "none",
      },
      effectColor: { r: 255, g: 170, b: 90 },
      audio: {
        device: null,
        bands: 16,
        gain: 1,
        floorDb: -62,
        ceilingDb: -12,
        attackMs: 18,
        releaseMs: 220,
        beatSensitivity: 1.4,
        beatBoost: 0.35,
        scrollSpeed: 0.08,
        effect: "spectrum",
        palette: { fromHue: 0.62, toHue: 0.08, saturation: 1 },
      },
      capture: {
        backend: "auto",
        targetFps: 60,
        adaptive: true,
        idleFps: 10,
        idleThreshold: 0.004,
        hdr: false,
        detectBlackBars: true,
        sampleGrid: 16,
      },
    },
  ],
};

let current: AppConfig = structuredClone(config);
const started = Date.now();

function hue(h: number): Rgb8 {
  const x = ((h % 1) + 1) % 1;
  const i = Math.floor(x * 6);
  const f = x * 6 - i;
  const q = 1 - f;
  const [r, g, b] = [
    [1, f, 0],
    [q, 1, 0],
    [0, 1, f],
    [0, q, 1],
    [f, 0, 1],
    [1, 0, q],
  ][i] ?? [1, 1, 1];
  return {
    r: Math.round(r * 210),
    g: Math.round(g * 210),
    b: Math.round(b * 210),
  };
}

function status(): EngineStatus {
  const count = current.profiles[0].layout.leds.length;
  const t = (Date.now() - started) / 4000;
  return {
    running: true,
    enabled: current.enabled,
    profile: current.profiles[0].name,
    profileId: current.profiles[0].id,
    mode: current.profiles[0].mode,
    fps: current.enabled ? 60 : 0,
    frameMs: 2.4,
    idle: false,
    ledCount: count,
    leds: current.enabled
      ? Array.from({ length: count }, (_, i) => hue(t + i / count))
      : Array.from({ length: count }, () => ({ r: 0, g: 0, b: 0 })),
    devices: [
      {
        label: "WLED 192.168.0.200",
        connected: true,
        reachable: true,
        error: null,
      },
    ],
    captureBackend: "Demo",
    display: "mock-1",
    sourceWidth: 2560,
    sourceHeight: 1440,
    insets: [0, 0, 0, 0],
    audioActive: false,
    audioEnergy: 0,
    frames: Math.floor((Date.now() - started) / 16),
    lastError: null,
  };
}

const displays: DisplayInfo[] = [
  {
    id: "mock-1",
    label: "Demo display",
    width: 2560,
    height: 1440,
    x: 0,
    y: 0,
    primary: true,
  },
];

const backends: BackendOption[] = [
  { kind: "auto", label: "Automatic" },
  { kind: "test", label: "Test pattern" },
];

const info: AppInfo = {
  version: "0.1.0",
  platform: "browser",
  configPath: "(browser preview, nothing is saved)",
  autostartSupported: false,
  serialSupported: false,
};

function buildLayout(params: WizardParams): Layout {
  const leds: LedSpec[] = [];
  const { top, right, bottom, left } = params.counts;
  const d = params.depth;
  const push = (x: number, y: number, w: number, h: number) =>
    leds.push({
      index: leds.length,
      display: params.display || "mock-1",
      rect: { x, y, w, h },
      weight: 1,
      enabled: true,
    });
  for (let i = 0; i < bottom; i++) push(i / bottom, 1 - d, 1 / bottom, d);
  for (let i = 0; i < right; i++) push(1 - d, 1 - (i + 1) / right, d, 1 / right);
  for (let i = 0; i < top; i++) push(1 - (i + 1) / top, 0, 1 / top, d);
  for (let i = 0; i < left; i++) push(0, i / left, d, 1 / left);
  return { ...edgeLayout(), leds, chain: params.chain };
}

const discovered: DiscoveredDevice[] = [
  {
    name: "wled-desk",
    host: "192.168.0.200",
    port: 21324,
    ledCount: 64,
    version: "0.15.0",
    kind: "wled",
  },
];

export const mockBackend = {
  get_config: async () => structuredClone(current),
  save_config: async (args: { config: AppConfig }) => {
    current = structuredClone(args.config);
    return structuredClone(current);
  },
  get_status: async () => status(),
  set_enabled: async (args: { enabled: boolean }) => {
    current.enabled = args.enabled;
  },
  set_active_profile: async (args: { id: string }) => {
    current.activeProfile = args.id;
    return structuredClone(current);
  },
  delete_profile: async () => structuredClone(current),
  list_displays: async () => displays,
  list_capture_backends: async () => backends,
  list_audio_devices: async () => ["Speakers (demo)"],
  discover_devices: async () => discovered,
  probe_device: async () => discovered[0],
  build_layout: async (args: { params: WizardParams }) =>
    buildLayout(args.params),
  identify_led: async () => {},
  hold_color: async () => {},
  set_launch_at_login: async () => {},
  app_info: async () => info,
  open_config_dir: async () => {},
} as Record<string, (args: never) => Promise<unknown>>;
