import { useEffect, useState } from "react";
import {
  Broadcast,
  Code,
  Crosshair,
  Gauge,
  GridFour,
  SlidersHorizontal,
  TreeStructure,
  Wrench,
} from "@phosphor-icons/react";

import { Badge, Mark, Toggle } from "./components/ui";
import { api } from "./lib/api";
import { useMasLight } from "./lib/store";
import type { ScriptExample } from "./lib/types";
import { Calibration } from "./screens/Calibration";
import { Dashboard } from "./screens/Dashboard";
import { Devices } from "./screens/Devices";
import { LayoutStudio } from "./screens/LayoutStudio";
import { Developer } from "./screens/Developer";
import { Rules } from "./screens/Rules";
import { Settings } from "./screens/Settings";
import { Studio } from "./screens/Studio";
import type { MessageKey } from "./lib/i18n";

type Screen =
  | "dashboard"
  | "layout"
  | "devices"
  | "calibration"
  | "studio"
  | "rules"
  | "developer"
  | "settings";

const NAV: { id: Screen; key: MessageKey; icon: typeof Gauge }[] = [
  { id: "dashboard", key: "nav.dashboard", icon: Gauge },
  { id: "layout", key: "nav.layout", icon: GridFour },
  { id: "devices", key: "nav.devices", icon: Broadcast },
  { id: "calibration", key: "nav.calibration", icon: Crosshair },
  { id: "studio", key: "nav.studio", icon: SlidersHorizontal },
  { id: "rules", key: "nav.rules", icon: TreeStructure },
  { id: "developer", key: "nav.developer", icon: Code },
  { id: "settings", key: "nav.settings", icon: Wrench },
];

export default function App() {
  const store = useMasLight();
  // The window can be opened straight onto a screen with a fragment, as in
  // #devices. The tray menu uses it to jump to a specific page, and the
  // documentation screenshots use it to photograph each one without a human
  // clicking through the sidebar.
  const [screen, setScreen] = useState<Screen>(() => {
    const wanted = window.location.hash.replace("#", "");
    return NAV.some((n) => n.id === wanted) ? (wanted as Screen) : "dashboard";
  });
  const [examples, setExamples] = useState<ScriptExample[]>([]);

  useEffect(() => {
    api.listScriptExamples().then(setExamples).catch(() => setExamples([]));
  }, []);
  const { t, config, status, loading, error } = store;

  if (loading) {
    return (
      <div className="app">
        <aside className="sidebar">
          <div className="brand">
            <Mark />
            <span className="brand-name">
              <b>Mas</b>
              <span>Light</span>
            </span>
          </div>
          <div className="nav">
            {NAV.map((n) => (
              <div
                key={n.id}
                className="skeleton"
                style={{ height: 30, margin: "1px 0" }}
              />
            ))}
          </div>
        </aside>
        <main className="main">
          <div className="content">
            <div className="stack">
              <div className="skeleton" style={{ height: 180 }} />
              <div className="skeleton" style={{ height: 260 }} />
            </div>
          </div>
        </main>
      </div>
    );
  }

  if (error && !config) {
    return (
      <div className="app" style={{ gridTemplateColumns: "1fr" }}>
        <main className="main">
          <div className="content">
            <div className="empty" style={{ maxWidth: 520, margin: "10vh auto" }}>
              <Mark size={40} />
              <h3>MasLight</h3>
              <p>{error}</p>
              <button className="btn btn-primary" onClick={store.reload}>
                {t("common.retry")}
              </button>
            </div>
          </div>
        </main>
      </div>
    );
  }

  if (!config) return null;

  const title = t(NAV.find((n) => n.id === screen)!.key);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <Mark />
          <span className="brand-name">
            <b>Mas</b>
            <span>Light</span>
          </span>
        </div>

        <nav className="nav">
          {NAV.map((item) => {
            const Icon = item.icon;
            return (
              // Below 900px the sidebar collapses and the CSS hides the
              // label, which would leave a button with nothing but a hidden
              // icon inside it and no name at all. The aria-label survives
              // that, and the title gives the icon rail the tooltip it needs
              // for anyone who cannot guess the glyph.
              <button
                key={item.id}
                className="nav-item"
                aria-current={screen === item.id ? "page" : undefined}
                aria-label={t(item.key)}
                title={t(item.key)}
                onClick={() => setScreen(item.id)}
              >
                <Icon size={17} weight={screen === item.id ? "fill" : "regular"} />
                <span>{t(item.key)}</span>
              </button>
            );
          })}
        </nav>

        <div className="sidebar-foot">
          <span>{store.info?.version ? `v${store.info.version}` : ""}</span>
          {status?.running ? (
            <Badge tone={status.enabled ? "ok" : "neutral"}>
              <span className="dot" />
              {status.enabled ? t("common.on") : t("common.off")}
            </Badge>
          ) : (
            <Badge tone="bad">
              <span className="dot" />
              {t("header.engineOff")}
            </Badge>
          )}
        </div>
      </aside>

      <main className="main">
        <header className="header">
          <h1>{title}</h1>
          <div className="header-spacer" />

          <select
            aria-label={t("common.profile")}
            style={{ width: 180 }}
            value={config.activeProfile}
            onChange={(e) =>
              store.update({ ...config, activeProfile: e.target.value })
            }
          >
            {config.profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>

          <Toggle
            label={t("header.lights")}
            checked={config.enabled}
            onChange={store.setEnabled}
          />
        </header>

        <div className="content">
          {screen === "dashboard" && (
            <Dashboard store={store} onGoToDevices={() => setScreen("devices")} />
          )}
          {screen === "layout" && <LayoutStudio store={store} />}
          {screen === "devices" && <Devices store={store} />}
          {screen === "calibration" && <Calibration store={store} />}
          {screen === "studio" && <Studio store={store} />}
          {screen === "rules" && <Rules store={store} />}
          {screen === "developer" && (
            <Developer store={store} examples={examples} />
          )}
          {screen === "settings" && <Settings store={store} />}
        </div>
      </main>

      {store.notice && <div className="toast">{store.notice}</div>}
    </div>
  );
}
