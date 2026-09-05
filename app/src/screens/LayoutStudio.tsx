import { useEffect, useState } from "react";
import { Lightning, MagicWand } from "@phosphor-icons/react";

import { LayoutCanvas } from "../components/preview";
import {
  Card,
  EmptyState,
  Field,
  NumberInput,
  Slider,
  Toggle,
} from "../components/ui";
import { api } from "../lib/api";
import type { Store } from "../lib/store";
import type { ColorOrder, Corner, Direction, DisplayInfo, WizardParams } from "../lib/types";

export function LayoutStudio({ store }: { store: Store }) {
  const { t, profile, updateProfile, status, setNotice } = store;
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [wizard, setWizard] = useState<WizardParams | null>(null);

  useEffect(() => {
    api.listDisplays().then(setDisplays).catch(() => setDisplays([]));
  }, []);

  useEffect(() => {
    if (!profile || wizard) return;
    const display =
      profile.layout.leds.find((l) => l.display)?.display ??
      displays.find((d) => d.primary)?.id ??
      displays[0]?.id ??
      "";
    setWizard({
      name: profile.name,
      display,
      counts: { top: 20, right: 12, bottom: 20, left: 12 },
      startCorner: "bottom-left",
      direction: "counter-clockwise",
      depth: 0.12,
      span: 1,
      chainOffset: 0,
      reverseChain: false,
      chain: profile.layout.chain,
    });
  }, [profile, displays, wizard]);

  if (!profile || !wizard) return null;

  const leds = profile.layout.leds;
  const led = selected === null ? null : leds.find((l) => l.index === selected);

  const generate = async () => {
    const layout = await api.buildLayout(wizard);
    updateProfile((p) => ({ ...p, layout }));
    setSelected(null);
    setNotice(`${layout.leds.length} LED`);
  };

  const moveLed = (index: number, x: number, y: number) => {
    updateProfile((p) => ({
      ...p,
      layout: {
        ...p.layout,
        leds: p.layout.leds.map((l) =>
          l.index === index
            ? {
                ...l,
                rect: {
                  ...l.rect,
                  x: Math.min(1 - l.rect.w, Math.max(0, x - l.rect.w / 2)),
                  y: Math.min(1 - l.rect.h, Math.max(0, y - l.rect.h / 2)),
                },
              }
            : l
        ),
      },
    }));
  };

  const patchLed = (index: number, patch: Partial<(typeof leds)[number]>) => {
    updateProfile((p) => ({
      ...p,
      layout: {
        ...p.layout,
        leds: p.layout.leds.map((l) =>
          l.index === index ? { ...l, ...patch } : l
        ),
      },
    }));
  };

  const setCount = (edge: keyof WizardParams["counts"], value: number) =>
    setWizard({
      ...wizard,
      counts: { ...wizard.counts, [edge]: Math.max(0, Math.round(value)) },
    });

  const total =
    wizard.counts.top +
    wizard.counts.right +
    wizard.counts.bottom +
    wizard.counts.left;

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "minmax(0, 1fr) 320px",
        gap: "var(--s-5)",
        alignItems: "start",
      }}
    >
      <div className="stack" style={{ maxWidth: "none" }}>
        <Card
          title={t("layout.title")}
          subtitle={t("layout.selectHint")}
          actions={
            <span className="mono" style={{ color: "var(--ink-subtle)" }}>
              {leds.length} LED
            </span>
          }
        >
          {leds.length === 0 ? (
            <EmptyState
              icon={<MagicWand size={26} />}
              title={t("layout.empty")}
              body={t("layout.emptyBody")}
            />
          ) : (
            <LayoutCanvas
              leds={leds}
              colors={status?.leds}
              selected={selected}
              onSelect={setSelected}
              onMove={moveLed}
            />
          )}
        </Card>

        {led && (
          <Card title={`${t("layout.selected")} ${led.index}`}>
            <div className="grid-2">
              <Field label={t("layout.display")}>
                <select
                  value={led.display}
                  onChange={(e) =>
                    patchLed(led.index, { display: e.target.value })
                  }
                >
                  <option value="">{t("layout.effectOnly")}</option>
                  {displays.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.label} ({d.width}x{d.height})
                    </option>
                  ))}
                  {!displays.some((d) => d.id === led.display) &&
                    led.display && (
                      <option value={led.display}>{led.display}</option>
                    )}
                </select>
              </Field>
              <Field label={t("layout.weight")}>
                <NumberInput
                  value={Number(led.weight.toFixed(2))}
                  min={0}
                  max={4}
                  step={0.1}
                  onChange={(v) => patchLed(led.index, { weight: v })}
                />
              </Field>
            </div>
            <div
              className="row row-wrap"
              style={{ marginTop: "var(--s-4)", gap: "var(--s-4)" }}
            >
              <Toggle
                label={t("layout.enabled")}
                checked={led.enabled}
                onChange={(v) => patchLed(led.index, { enabled: v })}
              />
              <button
                className="btn btn-sm"
                onClick={() => api.identifyLed(led.index, 1500)}
              >
                <Lightning size={14} weight="fill" />
                {t("layout.identify")}
              </button>
            </div>
          </Card>
        )}
      </div>

      <Card title={t("layout.wizard")} subtitle={t("layout.wizardHint")} tight>
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <Field label={t("layout.display")}>
            <select
              value={wizard.display}
              onChange={(e) => setWizard({ ...wizard, display: e.target.value })}
            >
              {displays.length === 0 && <option value="">-</option>}
              {displays.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.label}
                </option>
              ))}
            </select>
          </Field>

          <div className="grid-2">
            <Field label={t("layout.top")}>
              <NumberInput
                value={wizard.counts.top}
                min={0}
                max={500}
                onChange={(v) => setCount("top", v)}
              />
            </Field>
            <Field label={t("layout.bottom")}>
              <NumberInput
                value={wizard.counts.bottom}
                min={0}
                max={500}
                onChange={(v) => setCount("bottom", v)}
              />
            </Field>
            <Field label={t("layout.left")}>
              <NumberInput
                value={wizard.counts.left}
                min={0}
                max={500}
                onChange={(v) => setCount("left", v)}
              />
            </Field>
            <Field label={t("layout.right")}>
              <NumberInput
                value={wizard.counts.right}
                min={0}
                max={500}
                onChange={(v) => setCount("right", v)}
              />
            </Field>
          </div>

          <Field label={t("layout.startCorner")}>
            <select
              value={wizard.startCorner}
              onChange={(e) =>
                setWizard({ ...wizard, startCorner: e.target.value as Corner })
              }
            >
              <option value="bottom-left">{t("layout.cornerBottomLeft")}</option>
              <option value="bottom-right">
                {t("layout.cornerBottomRight")}
              </option>
              <option value="top-left">{t("layout.cornerTopLeft")}</option>
              <option value="top-right">{t("layout.cornerTopRight")}</option>
            </select>
          </Field>

          <Field label={t("layout.direction")}>
            <select
              value={wizard.direction}
              onChange={(e) =>
                setWizard({
                  ...wizard,
                  direction: e.target.value as Direction,
                })
              }
            >
              <option value="counter-clockwise">
                {t("layout.counterClockwise")}
              </option>
              <option value="clockwise">{t("layout.clockwise")}</option>
            </select>
          </Field>

          <Slider
            label={t("layout.depth")}
            hint={t("layout.depthHint")}
            value={Math.round(wizard.depth * 100)}
            min={2}
            max={45}
            format={(v) => `${v}%`}
            onChange={(v) => setWizard({ ...wizard, depth: v / 100 })}
          />

          <Slider
            label={t("layout.span")}
            value={Math.round(wizard.span * 100)}
            min={20}
            max={100}
            format={(v) => `${v}%`}
            onChange={(v) => setWizard({ ...wizard, span: v / 100 })}
          />

          <Field label={t("layout.offset")} hint={t("layout.offsetHint")}>
            <NumberInput
              value={wizard.chainOffset}
              min={-500}
              max={500}
              onChange={(v) =>
                setWizard({ ...wizard, chainOffset: Math.round(v) })
              }
            />
          </Field>

          <Toggle
            label={t("layout.reverse")}
            checked={wizard.reverseChain}
            onChange={(v) => setWizard({ ...wizard, reverseChain: v })}
          />

          <Field label={t("layout.colorOrder")}>
            <select
              value={wizard.chain.colorOrder}
              onChange={(e) => {
                const colorOrder = e.target.value as ColorOrder;
                setWizard({
                  ...wizard,
                  chain: { ...wizard.chain, colorOrder },
                });
                updateProfile((p) => ({
                  ...p,
                  layout: {
                    ...p.layout,
                    chain: { ...p.layout.chain, colorOrder },
                  },
                }));
              }}
            >
              {(["grb", "rgb", "rbg", "gbr", "brg", "bgr"] as ColorOrder[]).map(
                (o) => (
                  <option key={o} value={o}>
                    {o.toUpperCase()}
                  </option>
                )
              )}
            </select>
          </Field>

          <button className="btn btn-primary" onClick={generate}>
            <MagicWand size={15} weight="fill" />
            {t("layout.generate")} ({total})
          </button>
        </div>
      </Card>
    </div>
  );
}
