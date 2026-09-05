import { useEffect, useState } from "react";
import { WaveSine } from "@phosphor-icons/react";

import { Card, EmptyState, Field, Segmented, Slider, Toggle } from "../components/ui";
import { api } from "../lib/api";
import type { Store } from "../lib/store";
import type {
  BackendOption,
  CaptureBackendKind,
  LightMode,
  LuminancePolicy,
} from "../lib/types";

export function Studio({ store }: { store: Store }) {
  const { t, profile, updateProfile } = store;
  const [backends, setBackends] = useState<BackendOption[]>([]);

  useEffect(() => {
    api.listCaptureBackends().then(setBackends).catch(() => setBackends([]));
  }, []);

  if (!profile) return null;
  const { color, capture } = profile;

  const setColor = (patch: Partial<typeof color>) =>
    updateProfile((p) => ({ ...p, color: { ...p.color, ...patch } }));
  const setCapture = (patch: Partial<typeof capture>) =>
    updateProfile((p) => ({ ...p, capture: { ...p.capture, ...patch } }));

  // A full-white strip is the worst case, which is what a power supply has to
  // survive: three channels per LED at the configured current.
  const fullWhiteAmps =
    (profile.layout.leds.length * 3 * color.ledChannelMa) / 1000;

  return (
    <div className="stack">
      <Card title={t("studio.mode")}>
        <Segmented<LightMode>
          value={profile.mode}
          onChange={(mode) => updateProfile((p) => ({ ...p, mode }))}
          options={[
            { value: "screen", label: t("studio.modeScreen") },
            { value: "audio", label: t("studio.modeAudio") },
            { value: "effect", label: t("studio.modeEffect") },
            { value: "off", label: t("studio.modeOff") },
          ]}
        />
        {profile.mode === "audio" && (
          <div style={{ marginTop: "var(--s-4)" }}>
            <EmptyState
              icon={<WaveSine size={26} />}
              title={t("studio.audioSoon")}
              body={t("studio.audioSoonBody")}
            />
          </div>
        )}
      </Card>

      <div className="grid-2">
        <Card title={t("studio.picture")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("dash.brightness")}
              value={Math.round(color.brightness * 100)}
              min={0}
              max={100}
              format={(v) => `${v}%`}
              onChange={(v) => setColor({ brightness: v / 100 })}
            />
            <Slider
              label={t("studio.gamma")}
              value={Number(color.gamma.toFixed(2))}
              min={0.4}
              max={3}
              step={0.05}
              onChange={(v) => setColor({ gamma: v })}
            />
            <Slider
              label={t("dash.saturation")}
              value={Math.round(color.saturation * 100)}
              min={0}
              max={200}
              format={(v) => `${v}%`}
              onChange={(v) => setColor({ saturation: v / 100 })}
            />
            <Slider
              label={t("studio.vibrance")}
              value={Math.round(color.vibrance * 100)}
              min={0}
              max={100}
              format={(v) => `${v}%`}
              onChange={(v) => setColor({ vibrance: v / 100 })}
            />
            <Slider
              label={t("dash.temperature")}
              value={Math.round(color.temperatureK)}
              min={2000}
              max={10000}
              step={50}
              format={(v) => `${v} K`}
              onChange={(v) => setColor({ temperatureK: v })}
            />
          </div>
        </Card>

        <Card title={t("studio.motion")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("dash.smoothing")}
              value={Math.round(color.smoothingMs)}
              min={0}
              max={600}
              step={10}
              format={(v) => `${v} ms`}
              onChange={(v) => setColor({ smoothingMs: v })}
            />
            <Field label={t("studio.slew")}>
              <div className="row">
                <Toggle
                  label=""
                  checked={color.maxSlewPerS !== null}
                  onChange={(on) =>
                    setColor({ maxSlewPerS: on ? 4 : null })
                  }
                />
                <input
                  type="range"
                  min={0.5}
                  max={20}
                  step={0.5}
                  disabled={color.maxSlewPerS === null}
                  value={color.maxSlewPerS ?? 4}
                  aria-label={t("studio.slew")}
                  onChange={(e) =>
                    setColor({ maxSlewPerS: Number(e.target.value) })
                  }
                />
                <output className="mono">
                  {color.maxSlewPerS === null
                    ? t("common.off")
                    : color.maxSlewPerS.toFixed(1)}
                </output>
              </div>
            </Field>
            <Toggle
              label={t("studio.dither")}
              checked={color.dithering}
              onChange={(v) => setColor({ dithering: v })}
            />
            <span className="hint">{t("studio.ditherHint")}</span>
            <Slider
              label={t("cal.latency")}
              value={profile.latencyMs}
              min={0}
              max={400}
              step={5}
              format={(v) => `${v} ms`}
              onChange={(v) =>
                updateProfile((p) => ({ ...p, latencyMs: Math.round(v) }))
              }
            />
          </div>
        </Card>

        <Card title={t("studio.eyecare")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("studio.threshold")}
              value={Math.round(color.luminanceThreshold * 100)}
              min={0}
              max={30}
              format={(v) => `${v}%`}
              onChange={(v) => setColor({ luminanceThreshold: v / 100 })}
            />
            <Field label={t("studio.policy")}>
              <Segmented<LuminancePolicy>
                value={color.luminancePolicy}
                onChange={(luminancePolicy) => setColor({ luminancePolicy })}
                options={[
                  { value: "minimum-level", label: t("studio.policyMin") },
                  { value: "dead-zone", label: t("studio.policyDead") },
                ]}
              />
            </Field>
            <Toggle
              label={t("studio.hdr")}
              checked={color.hdrToneMap}
              onChange={(v) => setColor({ hdrToneMap: v })}
            />
            <span className="hint">{t("studio.hdrHint")}</span>
          </div>
        </Card>

        <Card title={t("studio.power")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Field label={t("studio.powerLimit")} hint={t("studio.powerHint")}>
              <div className="row">
                <Toggle
                  label=""
                  checked={color.powerLimitA !== null}
                  onChange={(on) =>
                    setColor({ powerLimitA: on ? 2 : null })
                  }
                />
                <input
                  type="range"
                  min={0.2}
                  max={20}
                  step={0.1}
                  disabled={color.powerLimitA === null}
                  value={color.powerLimitA ?? 2}
                  aria-label={t("studio.powerLimit")}
                  onChange={(e) =>
                    setColor({ powerLimitA: Number(e.target.value) })
                  }
                />
                <output className="mono">
                  {color.powerLimitA === null
                    ? t("common.off")
                    : `${color.powerLimitA.toFixed(1)} A`}
                </output>
              </div>
            </Field>
            <Slider
              label={t("studio.channelMa")}
              value={Math.round(color.ledChannelMa)}
              min={5}
              max={40}
              format={(v) => `${v} mA`}
              onChange={(v) => setColor({ ledChannelMa: v })}
            />
            <div className="row" style={{ justifyContent: "space-between" }}>
              <span className="hint">{t("studio.estimated")}</span>
              <span className="mono">{fullWhiteAmps.toFixed(2)} A</span>
            </div>
          </div>
        </Card>
      </div>

      <Card title={t("studio.capture")}>
        <div className="grid-2">
          <Field label={t("studio.backend")}>
            <select
              value={capture.backend}
              onChange={(e) =>
                setCapture({ backend: e.target.value as CaptureBackendKind })
              }
            >
              {backends.map((b) => (
                <option key={b.kind} value={b.kind}>
                  {b.label}
                </option>
              ))}
            </select>
          </Field>
          <Slider
            label={t("studio.fps")}
            value={capture.targetFps}
            min={10}
            max={144}
            format={(v) => `${v} fps`}
            onChange={(v) => setCapture({ targetFps: Math.round(v) })}
          />
          <Slider
            label={t("studio.idleFps")}
            value={capture.idleFps}
            min={1}
            max={Math.max(1, capture.targetFps)}
            format={(v) => `${v} fps`}
            onChange={(v) => setCapture({ idleFps: Math.round(v) })}
          />
          <Slider
            label={t("studio.grid")}
            hint={t("studio.gridHint")}
            value={capture.sampleGrid}
            min={4}
            max={48}
            step={2}
            onChange={(v) => setCapture({ sampleGrid: Math.round(v) })}
          />
        </div>
        <div
          className="row row-wrap"
          style={{ marginTop: "var(--s-4)", gap: "var(--s-5)" }}
        >
          <Toggle
            label={t("studio.adaptive")}
            checked={capture.adaptive}
            onChange={(v) => setCapture({ adaptive: v })}
          />
          <Toggle
            label={t("studio.bars")}
            checked={capture.detectBlackBars}
            onChange={(v) => setCapture({ detectBlackBars: v })}
          />
        </div>
      </Card>
    </div>
  );
}
