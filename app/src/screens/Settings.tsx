import { FolderOpen, Plus, Trash } from "@phosphor-icons/react";

import { Alert, Card, Field, Segmented, Toggle } from "../components/ui";
import { api } from "../lib/api";
import type { Store } from "../lib/store";

export function Settings({ store }: { store: Store }) {
  const { t, config, info, update, setNotice } = store;
  if (!config) return null;

  const setUi = (patch: Partial<typeof config.ui>) =>
    update({ ...config, ui: { ...config.ui, ...patch } });

  const addProfile = () => {
    const id = `profile-${Date.now().toString(36)}`;
    const base = config.profiles.find((p) => p.id === config.activeProfile);
    if (!base) return;
    update({
      ...config,
      profiles: [
        ...config.profiles,
        { ...base, id, name: `${base.name} 2` },
      ],
      activeProfile: id,
    });
    setNotice(t("settings.newProfile"));
  };

  const removeProfile = (id: string) => {
    if (config.profiles.length <= 1) return;
    api
      .deleteProfile(id)
      .then((next) => update(next))
      .catch(() => {});
  };

  return (
    <div className="stack">
      <Card title={t("settings.appearance")}>
        <div className="grid-2">
          <Field label={t("settings.language")}>
            <Segmented
              value={config.ui.language}
              onChange={(language) => setUi({ language })}
              options={[
                { value: "system", label: t("common.automatic") },
                { value: "tr", label: "Turkce" },
                { value: "en", label: "English" },
              ]}
            />
          </Field>
          <Field label={t("settings.theme")}>
            <Segmented
              value={config.ui.theme}
              onChange={(theme) => setUi({ theme })}
              options={[
                { value: "dark", label: t("settings.themeDark") },
                { value: "light", label: t("settings.themeLight") },
                { value: "system", label: t("settings.themeSystem") },
              ]}
            />
          </Field>
        </div>
      </Card>

      <Card title={t("settings.startup")}>
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <Toggle
            label={t("settings.launchAtLogin")}
            checked={config.ui.launchAtLogin}
            onChange={(v) => {
              api
                .setLaunchAtLogin(v)
                .then(() => setUi({ launchAtLogin: v }))
                .catch(() => setNotice("?"));
            }}
          />
          <Toggle
            label={t("settings.startMinimised")}
            checked={config.ui.startMinimised}
            onChange={(v) => setUi({ startMinimised: v })}
          />
        </div>
      </Card>

      <Card title={t("settings.privacy")}>
        <Alert>{t("settings.offline")}</Alert>
        <div style={{ marginTop: "var(--s-4)" }}>
          <Toggle
            label={t("settings.updates")}
            checked={config.ui.checkForUpdates}
            onChange={(v) => setUi({ checkForUpdates: v })}
          />
          <p className="hint" style={{ marginTop: 6 }}>
            {t("settings.updatesHint")}
          </p>
        </div>
      </Card>

      <Card title={t("settings.profiles")}>
        <div className="list">
          {config.profiles.map((p) => (
            <div className="list-row" key={p.id}>
              <div className="list-row-main">
                <input
                  type="text"
                  value={p.name}
                  aria-label={t("settings.rename")}
                  onChange={(e) =>
                    update({
                      ...config,
                      profiles: config.profiles.map((q) =>
                        q.id === p.id ? { ...q, name: e.target.value } : q
                      ),
                    })
                  }
                />
                <div style={{ marginTop: 4 }}>
                  {p.layout.leds.length} LED, {p.devices.length}{" "}
                  {t("nav.devices").toLowerCase()}
                </div>
              </div>
              <button
                className="btn btn-sm btn-danger"
                disabled={config.profiles.length <= 1}
                onClick={() => removeProfile(p.id)}
                aria-label={t("common.delete")}
              >
                <Trash size={14} />
              </button>
            </div>
          ))}
        </div>
        <div className="row" style={{ marginTop: "var(--s-4)" }}>
          <button className="btn" onClick={addProfile}>
            <Plus size={14} weight="bold" />
            {t("settings.newProfile")}
          </button>
        </div>
      </Card>

      <Card title={t("settings.about")}>
        <div style={{ display: "grid", gap: "var(--s-3)" }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="hint">MasLight</span>
            <span className="mono">
              {info?.version ?? "0.1.0"} ({info?.platform ?? "-"})
            </span>
          </div>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <span className="hint">{t("settings.configFile")}</span>
            <span className="mono" data-selectable>
              {info?.configPath ?? "-"}
            </span>
          </div>
          <div className="row">
            <button className="btn btn-sm" onClick={() => api.openConfigDir()}>
              <FolderOpen size={14} />
              {t("settings.openFolder")}
            </button>
          </div>
          <p className="hint">{t("settings.license")}</p>
        </div>
      </Card>
    </div>
  );
}
