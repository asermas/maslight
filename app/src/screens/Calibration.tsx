import { useEffect, useState } from "react";
import { CheckCircle } from "@phosphor-icons/react";

import { Alert, Card, Field, Slider } from "../components/ui";
import { api } from "../lib/api";
import type { Store } from "../lib/store";
import type { ColorOrder } from "../lib/types";

type Step = "order" | "white" | "latency";

/**
 * The calibration wizard.
 *
 * Every step drives the strip directly and asks the person what they see. That
 * is the only reliable way to learn things the controller cannot report:
 * channel order, how the LEDs render white, and how far ahead of the panel the
 * strip is.
 */
export function Calibration({ store }: { store: Store }) {
  const { t, profile, updateProfile, setNotice } = store;
  const [step, setStep] = useState<Step>("order");

  // Always release the strip when the screen is left, otherwise a held colour
  // would outlive the wizard.
  useEffect(() => () => void api.holdColor(null).catch(() => {}), []);

  if (!profile) return null;
  const color = profile.color;

  const steps: { id: Step; label: string }[] = [
    { id: "order", label: t("cal.stepOrder") },
    { id: "white", label: t("cal.stepWhite") },
    { id: "latency", label: t("cal.stepLatency") },
  ];

  const setOrder = (order: ColorOrder) => {
    updateProfile((p) => ({
      ...p,
      layout: { ...p.layout, chain: { ...p.layout.chain, colorOrder: order } },
    }));
    setNotice(`${t("cal.orderSet")} ${order.toUpperCase()}`);
    setStep("white");
    api.holdColor("#ffffff").catch(() => {});
  };

  const setGain = (channel: 0 | 1 | 2, value: number) => {
    updateProfile((p) => {
      const gain = [...p.color.channelGain] as [number, number, number];
      gain[channel] = value;
      return { ...p, color: { ...p.color, channelGain: gain } };
    });
  };

  return (
    <div className="stack">
      <div className="steps">
        {steps.map((s, i) => (
          <button
            key={s.id}
            className="step"
            aria-current={step === s.id ? "step" : undefined}
            onClick={() => setStep(s.id)}
          >
            <span className="step-index">{i + 1}</span>
            {s.label}
          </button>
        ))}
      </div>

      {step === "order" && (
        <Card title={t("cal.stepOrder")} subtitle={t("cal.orderBody")}>
          <div className="row row-wrap">
            <button
              className="btn btn-primary"
              onClick={() => api.holdColor("#ff0000")}
            >
              {t("cal.sendRed")}
            </button>
            <span className="hint">
              {t("layout.colorOrder")}:{" "}
              <span className="mono">
                {profile.layout.chain.colorOrder.toUpperCase()}
              </span>
            </span>
          </div>

          <div
            className="row row-wrap"
            style={{ marginTop: "var(--s-5)", gap: "var(--s-3)" }}
          >
            <button className="btn" onClick={() => setOrder("rgb")}>
              {t("cal.showsRed")}
            </button>
            <button className="btn" onClick={() => setOrder("grb")}>
              {t("cal.showsGreen")}
            </button>
            <button className="btn" onClick={() => setOrder("bgr")}>
              {t("cal.showsBlue")}
            </button>
          </div>
        </Card>
      )}

      {step === "white" && (
        <Card title={t("cal.stepWhite")} subtitle={t("cal.whiteBody")}>
          <div className="row" style={{ alignItems: "flex-start", gap: "var(--s-5)" }}>
            <div style={{ flex: "0 0 180px" }}>
              <div
                aria-label={t("cal.patch")}
                style={{
                  width: "100%",
                  height: 120,
                  borderRadius: "var(--r-md)",
                  background: "#ffffff",
                  border: "1px solid var(--hairline)",
                }}
              />
              <p className="hint" style={{ marginTop: 8 }}>
                {t("cal.patch")}
              </p>
            </div>

            <div style={{ flex: 1, display: "grid", gap: "var(--s-4)" }}>
              <button
                className="btn btn-primary"
                style={{ justifySelf: "start" }}
                onClick={() => api.holdColor("#ffffff")}
              >
                {t("cal.sendWhite")}
              </button>
              <Slider
                label={t("cal.gainR")}
                value={Math.round(color.channelGain[0] * 100)}
                min={20}
                max={200}
                format={(v) => `${v}%`}
                onChange={(v) => setGain(0, v / 100)}
              />
              <Slider
                label={t("cal.gainG")}
                value={Math.round(color.channelGain[1] * 100)}
                min={20}
                max={200}
                format={(v) => `${v}%`}
                onChange={(v) => setGain(1, v / 100)}
              />
              <Slider
                label={t("cal.gainB")}
                value={Math.round(color.channelGain[2] * 100)}
                min={20}
                max={200}
                format={(v) => `${v}%`}
                onChange={(v) => setGain(2, v / 100)}
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
            </div>
          </div>

          <div className="row" style={{ marginTop: "var(--s-5)" }}>
            <button
              className="btn btn-primary"
              onClick={() => {
                api.holdColor(null).catch(() => {});
                setStep("latency");
              }}
            >
              {t("common.next")}
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => api.holdColor(null)}
            >
              {t("cal.release")}
            </button>
          </div>
        </Card>
      )}

      {step === "latency" && (
        <Card title={t("cal.stepLatency")} subtitle={t("cal.latencyBody")}>
          <Field label={t("cal.latency")}>
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
          </Field>
          <div style={{ marginTop: "var(--s-4)" }}>
            <Alert icon={<CheckCircle size={15} weight="fill" />}>
              {t("cal.done")}
            </Alert>
          </div>
        </Card>
      )}
    </div>
  );
}
