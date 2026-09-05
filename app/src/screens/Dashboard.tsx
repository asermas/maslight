import {
  CirclesThree,
  Monitor,
  Plugs,
  Warning,
} from "@phosphor-icons/react";

import { StripPreview } from "../components/preview";
import {
  Alert,
  Badge,
  Card,
  DeviceBadge,
  EmptyState,
  Slider,
  Stat,
} from "../components/ui";
import type { Store } from "../lib/store";

export function Dashboard({
  store,
  onGoToDevices,
}: {
  store: Store;
  onGoToDevices: () => void;
}) {
  const { t, status, profile, config, updateProfile } = store;
  if (!profile || !config) return null;

  const color = profile.color;
  const hasBars = (status?.insets ?? [0, 0, 0, 0]).some((v) => v > 0.005);
  const noCapture =
    status?.running === true &&
    profile.mode === "screen" &&
    (!status.captureBackend || status.captureBackend === "none");

  return (
    <div className="stack">
      <Card
        title={t("dash.preview")}
        subtitle={t("dash.previewHint")}
        actions={
          // A screen profile with no capture backend is a problem, and saying
          // the screen is merely still would hide it.
          noCapture ? (
            <Badge tone="bad">{t("dash.noCapture")}</Badge>
          ) : status?.idle ? (
            <Badge tone="warn">{t("dash.idle")}</Badge>
          ) : null
        }
      >
        <StripPreview leds={status?.leds ?? []} />

        <div
          className="row row-wrap"
          style={{ marginTop: "var(--s-5)", gap: "var(--s-6)" }}
        >
          <Stat
            value={status ? status.fps.toFixed(0) : "0"}
            label={t("dash.fps")}
          />
          <Stat
            value={status ? `${status.frameMs.toFixed(1)} ms` : "0.0 ms"}
            label={t("dash.frame")}
          />
          <Stat value={profile.layout.leds.length} label={t("dash.leds")} />
          <Stat
            value={
              status && status.sourceWidth > 0
                ? `${status.sourceWidth}x${status.sourceHeight}`
                : "-"
            }
            label={t("dash.source")}
          />
          <Stat
            value={status?.captureBackend ?? "-"}
            label={t("dash.backend")}
          />
        </div>

        {hasBars && (
          <div style={{ marginTop: "var(--s-4)" }}>
            <Badge tone="neutral">
              <Monitor size={13} weight="bold" />
              {t("dash.bars")}
            </Badge>
          </div>
        )}

        {status && !status.enabled && (
          <div style={{ marginTop: "var(--s-4)" }}>
            <Alert icon={<CirclesThree size={15} />}>
              {t("dash.stopped")}. {t("dash.stoppedBody")}
            </Alert>
          </div>
        )}

        {status?.lastError && (
          <div style={{ marginTop: "var(--s-4)" }}>
            <Alert tone="bad" icon={<Warning size={15} weight="fill" />}>
              {status.lastError}
            </Alert>
          </div>
        )}
      </Card>

      <div className="grid-2">
        <Card title={t("dash.devices")}>
          {profile.devices.length === 0 ? (
            <EmptyState
              icon={<Plugs size={26} />}
              title={t("dash.noDevices")}
              body={t("dash.noDevicesBody")}
              action={
                <button className="btn btn-primary" onClick={onGoToDevices}>
                  {t("dash.addDevice")}
                </button>
              }
            />
          ) : (
            <div className="list">
              {(status?.devices ?? []).map((d, i) => (
                <div className="list-row" key={`${d.label}-${i}`}>
                  <div className="list-row-main">
                    <strong>{d.label}</strong>
                    {d.error && <div>{d.error}</div>}
                    {!d.error && d.reachable === false && (
                      <div>{t("devices.noAnswerHint")}</div>
                    )}
                  </div>
                  <DeviceBadge
                    connected={d.connected}
                    reachable={d.reachable}
                    labels={{
                      sending: t("devices.sending"),
                      noAnswer: t("devices.noAnswer"),
                      off: t("common.off"),
                    }}
                  />
                </div>
              ))}
              {(status?.devices?.length ?? 0) === 0 &&
                profile.devices.map((_, i) => (
                  <div className="list-row" key={i}>
                    <div className="list-row-main">
                      <strong>{t("common.loading")}</strong>
                    </div>
                  </div>
                ))}
            </div>
          )}
        </Card>

        <Card title={t("dash.quick")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("dash.brightness")}
              value={Math.round(color.brightness * 100)}
              min={0}
              max={100}
              format={(v) => `${v}%`}
              onChange={(v) =>
                updateProfile((p) => ({
                  ...p,
                  color: { ...p.color, brightness: v / 100 },
                }))
              }
            />
            <Slider
              label={t("dash.temperature")}
              value={Math.round(color.temperatureK)}
              min={2000}
              max={10000}
              step={50}
              format={(v) => `${v} K`}
              onChange={(v) =>
                updateProfile((p) => ({
                  ...p,
                  color: { ...p.color, temperatureK: v },
                }))
              }
            />
            <Slider
              label={t("dash.saturation")}
              value={Math.round(color.saturation * 100)}
              min={0}
              max={200}
              format={(v) => `${v}%`}
              onChange={(v) =>
                updateProfile((p) => ({
                  ...p,
                  color: { ...p.color, saturation: v / 100 },
                }))
              }
            />
            <Slider
              label={t("dash.smoothing")}
              value={Math.round(color.smoothingMs)}
              min={0}
              max={600}
              step={10}
              format={(v) => `${v} ms`}
              onChange={(v) =>
                updateProfile((p) => ({
                  ...p,
                  color: { ...p.color, smoothingMs: v },
                }))
              }
            />
          </div>
        </Card>
      </div>
    </div>
  );
}
