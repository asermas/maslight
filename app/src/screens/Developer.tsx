import { useState } from "react";
import { Code, Play, Warning } from "@phosphor-icons/react";

import { StripPreview } from "../components/preview";
import { Alert, Badge, Card, EmptyState } from "../components/ui";
import type { Store } from "../lib/store";

/**
 * The scripted effect editor.
 *
 * Deliberately plain: a textarea, the examples, the error, and the live strip.
 * A syntax highlighting editor would be a large dependency for a screen whose
 * real feedback is the strip itself.
 */
export function Developer({
  store,
  examples,
}: {
  store: Store;
  examples: { name: string; source: string }[];
}) {
  const { t, profile, updateProfile, status } = store;
  const [draft, setDraft] = useState<string | null>(null);

  if (!profile) return null;
  const source = draft ?? profile.script;
  const dirty = draft !== null && draft !== profile.script;
  const error = status?.scriptError ?? null;
  const running = profile.mode === "effect" && profile.script.trim().length > 0;

  const apply = () => {
    updateProfile((p) => ({ ...p, script: source, mode: "effect" }));
    setDraft(null);
  };

  return (
    <div className="stack">
      <Card
        title={t("dev.title")}
        subtitle={t("dev.body")}
        actions={
          <div className="row" style={{ gap: "var(--s-2)" }}>
            {running && !error && (
              <Badge tone="ok">
                <span className="dot" />
                {t("dev.running")}
              </Badge>
            )}
            <button className="btn btn-sm btn-primary" onClick={apply} disabled={!dirty}>
              <Play size={13} weight="fill" />
              {t("dev.run")}
            </button>
          </div>
        }
      >
        <textarea
          spellCheck={false}
          value={source}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={t("dev.placeholder")}
          className="mono"
          style={{
            width: "100%",
            minHeight: 320,
            resize: "vertical",
            lineHeight: 1.55,
            tabSize: 4,
            whiteSpace: "pre",
            overflowX: "auto",
          }}
        />

        {error && (
          <div style={{ marginTop: "var(--s-3)" }}>
            <Alert tone="bad" icon={<Warning size={15} weight="fill" />}>
              {error}
            </Alert>
          </div>
        )}

        <div style={{ marginTop: "var(--s-4)" }}>
          <span className="hint">{t("dash.preview")}</span>
          <div style={{ marginTop: 6 }}>
            <StripPreview leds={status?.leds ?? []} />
          </div>
        </div>
      </Card>

      <div className="grid-2">
        <Card title={t("dev.examples")}>
          {examples.length === 0 ? (
            <EmptyState icon={<Code size={26} />} title={t("dev.examples")} />
          ) : (
            <div className="list">
              {examples.map((example) => (
                <div className="list-row" key={example.name}>
                  <div className="list-row-main">
                    <strong>{example.name}</strong>
                  </div>
                  <button
                    className="btn btn-sm"
                    onClick={() => setDraft(example.source)}
                  >
                    {t("dev.load")}
                  </button>
                </div>
              ))}
            </div>
          )}
        </Card>

        <Card title={t("dev.reference")}>
          <div style={{ display: "grid", gap: "var(--s-2)", fontSize: 12.5 }}>
            {[
              ["ctx.n", t("dev.refN")],
              ["ctx.t", t("dev.refT")],
              ["ctx.dt", t("dev.refDt")],
              ["ctx.energy", t("dev.refEnergy")],
              ["ctx.beat", t("dev.refBeat")],
              ["ctx.pulse", t("dev.refPulse")],
              ["ctx.bands", t("dev.refBands")],
              ["hsv(h, s, v)", t("dev.refHsv")],
              ["rgb(r, g, b)", t("dev.refRgb")],
              ["mix(a, b, t)", t("dev.refMix")],
              ["wave(t)", t("dev.refWave")],
              ["clamp01(v)", t("dev.refClamp")],
            ].map(([name, description]) => (
              <div
                key={name}
                className="row"
                style={{ justifyContent: "space-between", gap: "var(--s-4)" }}
              >
                <span className="mono" style={{ color: "var(--accent)" }}>
                  {name}
                </span>
                <span className="hint" style={{ textAlign: "right" }}>
                  {description}
                </span>
              </div>
            ))}
          </div>
          <p className="hint" style={{ marginTop: "var(--s-4)" }}>
            {t("dev.sandbox")}
          </p>
        </Card>
      </div>
    </div>
  );
}
