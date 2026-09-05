import { useEffect, useState } from "react";

import { Badge, Card, Field, Segmented, Slider } from "../components/ui";
import { api } from "../lib/api";
import type { Store } from "../lib/store";
import type { AudioEffect, AudioSettings } from "../lib/types";

/**
 * The audio mode.
 *
 * Split out of Studio because it is a screen in its own right once the
 * spectrum is live: the meter at the top is what tells someone whether the
 * loopback device is the right one, and every control below it changes what
 * that meter does.
 */
export function AudioPanel({ store }: { store: Store }) {
  const { t, profile, updateProfile, status } = store;
  const [devices, setDevices] = useState<string[]>([]);

  useEffect(() => {
    api.listAudioDevices().then(setDevices).catch(() => setDevices([]));
  }, []);

  if (!profile) return null;
  const audio = profile.audio;
  const set = (patch: Partial<AudioSettings>) =>
    updateProfile((p) => ({ ...p, audio: { ...p.audio, ...patch } }));

  const energy = status?.audioEnergy ?? 0;
  const live = (status?.audioActive ?? false) && energy > 0.01;

  return (
    <>
      <Card
        title={t("studio.audioEffect")}
        actions={
          <Badge tone={live ? "ok" : "neutral"}>
            <span className="dot" />
            {live ? t("studio.audioLive") : t("studio.audioSilent")}
          </Badge>
        }
      >
        {/* A meter beats any amount of prose for "is it hearing anything". */}
        <div
          aria-label={t("studio.audioLive")}
          style={{
            height: 8,
            borderRadius: "var(--r-pill)",
            background: "var(--surface-3)",
            overflow: "hidden",
            marginBottom: "var(--s-5)",
          }}
        >
          <div
            style={{
              width: `${Math.round(Math.min(1, energy) * 100)}%`,
              height: "100%",
              background: "var(--accent)",
              transition: "width 80ms linear",
            }}
          />
        </div>

        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <Field label={t("studio.audioEffect")}>
            <Segmented<AudioEffect>
              value={audio.effect}
              onChange={(effect) => set({ effect })}
              options={[
                { value: "spectrum", label: t("studio.audioSpectrum") },
                { value: "energy", label: t("studio.audioEnergy") },
                { value: "wave", label: t("studio.audioWave") },
                { value: "scroll", label: t("studio.audioScroll") },
              ]}
            />
          </Field>

          <Field label={t("studio.audioDevice")}>
            <select
              value={audio.device ?? ""}
              onChange={(e) => set({ device: e.target.value || null })}
            >
              <option value="">{t("studio.audioDefault")}</option>
              {devices.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </Field>

          <Slider
            label={t("studio.audioBlend")}
            hint={t("studio.audioBlendHint")}
            value={Math.round(profile.audioBlend * 100)}
            min={0}
            max={100}
            format={(v) => `${v}%`}
            onChange={(v) =>
              updateProfile((p) => ({ ...p, audioBlend: v / 100 }))
            }
          />
        </div>
      </Card>

      <div className="grid-2">
        <Card title={t("studio.motion")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("studio.audioBands")}
              value={audio.bands}
              min={4}
              max={64}
              step={2}
              onChange={(v) => set({ bands: Math.round(v) })}
            />
            <Slider
              label={t("studio.audioAttack")}
              value={Math.round(audio.attackMs)}
              min={0}
              max={200}
              step={2}
              format={(v) => `${v} ms`}
              onChange={(v) => set({ attackMs: v })}
            />
            <Slider
              label={t("studio.audioRelease")}
              value={Math.round(audio.releaseMs)}
              min={20}
              max={1200}
              step={10}
              format={(v) => `${v} ms`}
              onChange={(v) => set({ releaseMs: v })}
            />
            <Slider
              label={t("studio.audioBeat")}
              value={Number(audio.beatSensitivity.toFixed(1))}
              min={0.4}
              max={4}
              step={0.1}
              onChange={(v) => set({ beatSensitivity: v })}
            />
            <Slider
              label={t("studio.audioBeatBoost")}
              value={Math.round(audio.beatBoost * 100)}
              min={0}
              max={100}
              format={(v) => `${v}%`}
              onChange={(v) => set({ beatBoost: v / 100 })}
            />
          </div>
        </Card>

        <Card title={t("studio.color")}>
          <div style={{ display: "grid", gap: "var(--s-4)" }}>
            <Slider
              label={t("studio.audioGain")}
              value={Number(audio.gain.toFixed(1))}
              min={0.2}
              max={4}
              step={0.1}
              format={(v) => `${v.toFixed(1)}x`}
              onChange={(v) => set({ gain: v })}
            />
            <Slider
              label={t("studio.audioFloor")}
              value={Math.round(audio.floorDb)}
              min={-90}
              max={-30}
              format={(v) => `${v} dB`}
              onChange={(v) => set({ floorDb: v })}
            />
            <Slider
              label={t("studio.audioCeiling")}
              value={Math.round(audio.ceilingDb)}
              min={-40}
              max={0}
              format={(v) => `${v} dB`}
              onChange={(v) => set({ ceilingDb: v })}
            />
            <HueSlider
              label={t("studio.audioHueFrom")}
              value={audio.palette.fromHue}
              onChange={(fromHue) =>
                set({ palette: { ...audio.palette, fromHue } })
              }
            />
            <HueSlider
              label={t("studio.audioHueTo")}
              value={audio.palette.toHue}
              onChange={(toHue) => set({ palette: { ...audio.palette, toHue } })}
            />
            <Slider
              label={t("dash.saturation")}
              value={Math.round(audio.palette.saturation * 100)}
              min={0}
              max={100}
              format={(v) => `${v}%`}
              onChange={(v) =>
                set({ palette: { ...audio.palette, saturation: v / 100 } })
              }
            />
          </div>
        </Card>
      </div>
    </>
  );
}

/** A hue slider that shows a swatch of the hue it is selecting. */
function HueSlider({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      <div className="slider-row">
        <input
          type="range"
          min={0}
          max={100}
          value={Math.round(value * 100)}
          aria-label={label}
          onChange={(e) => onChange(Number(e.target.value) / 100)}
        />
        <span
          aria-hidden
          style={{
            justifySelf: "end",
            width: 26,
            height: 18,
            borderRadius: "var(--r-xs)",
            border: "1px solid var(--hairline)",
            background: `hsl(${Math.round(value * 360)} 90% 55%)`,
          }}
        />
      </div>
    </div>
  );
}
