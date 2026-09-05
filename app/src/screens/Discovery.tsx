import { useEffect, useRef, useState } from "react";
import { Camera, CheckCircle, Warning } from "@phosphor-icons/react";

import { Alert, Badge, Card, Field } from "../components/ui";
import { api, errorText } from "../lib/api";
import { toLuma, type LumaPhoto } from "../lib/photo";
import type { Store } from "../lib/store";
import type { PlanStep } from "../lib/types";

type Phase = "idle" | "shooting" | "upload" | "corners" | "done";

/** Corner labels in the order the homography wants them. */
const CORNERS = ["cornerTL", "cornerTR", "cornerBR", "cornerBL"] as const;

/**
 * Camera assisted position discovery.
 *
 * Every other ambilight tool asks you to describe your strip. This asks the
 * strip: it lights a sequence of patterns, you photograph each one from one
 * fixed spot, and both the position and the chain order fall out of the
 * photographs.
 */
export function Discovery({ store }: { store: Store }) {
  const { t, profile, updateProfile, setNotice } = store;
  const [phase, setPhase] = useState<Phase>("idle");
  const [steps, setSteps] = useState<PlanStep[]>([]);
  const [current, setCurrent] = useState(0);
  const [photos, setPhotos] = useState<LumaPhoto[]>([]);
  const [corners, setCorners] = useState<[number, number][]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const imageRef = useRef<HTMLImageElement>(null);

  const ledCount = profile?.layout.leds.length ?? 0;

  // Never leave the strip stuck on a pattern.
  useEffect(() => () => void api.calibrationRelease().catch(() => {}), []);

  useEffect(() => {
    return () => photos.forEach((p) => URL.revokeObjectURL(p.url));
  }, [photos]);

  if (!profile) return null;

  const start = async () => {
    setError(null);
    try {
      const plan = await api.calibrationPlan(ledCount);
      setSteps(plan);
      setCurrent(0);
      setPhotos([]);
      setCorners([]);
      await api.calibrationShow(ledCount, 0);
      setPhase("shooting");
    } catch (e) {
      setError(errorText(e));
    }
  };

  const next = async () => {
    const step = current + 1;
    if (step >= steps.length) {
      await api.calibrationRelease().catch(() => {});
      setPhase("upload");
      return;
    }
    setCurrent(step);
    await api.calibrationShow(ledCount, step).catch(() => {});
  };

  const cancel = async () => {
    await api.calibrationRelease().catch(() => {});
    setPhase("idle");
  };

  const load = async (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      // Sorted by name, because a phone numbers photographs in the order they
      // were taken and that is the order the sequence needs.
      const sorted = Array.from(files).sort((a, b) => a.name.localeCompare(b.name));
      const loaded = await Promise.all(sorted.map(toLuma));
      setPhotos(loaded);
      if (loaded.length !== steps.length) {
        setError(t("disc.countMismatch"));
      } else {
        setPhase("corners");
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const markCorner = (event: React.MouseEvent<HTMLImageElement>) => {
    if (corners.length >= 4) return;
    const image = imageRef.current;
    if (!image) return;
    const box = image.getBoundingClientRect();
    // The photograph is shown scaled, so a click has to come back to the
    // pixels the decoder will actually read.
    const scaleX = (photos[1]?.width ?? image.naturalWidth) / box.width;
    const scaleY = (photos[1]?.height ?? image.naturalHeight) / box.height;
    setCorners([
      ...corners,
      [(event.clientX - box.left) * scaleX, (event.clientY - box.top) * scaleY],
    ]);
  };

  const build = async () => {
    if (corners.length !== 4) return;
    setBusy(true);
    setError(null);
    try {
      const display =
        profile.layout.leds.find((l) => l.display)?.display ??
        profile.layout.displays[0]?.id ??
        "";
      const result = await api.calibrationDecode({
        ledCount,
        photos: photos.map((p) => ({
          luma: p.luma,
          width: p.width,
          height: p.height,
        })),
        corners: corners.map(([x, y]) => [x, y]) as [number, number][],
        display,
        depth: 0.12,
      });
      updateProfile((p) => ({ ...p, layout: result.layout }));
      setNotice(`${result.found} / ${result.expected} LED`);
      setPhase("done");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const step = steps[current];
  const stepLabel = !step
    ? ""
    : step.kind === "off"
      ? t("disc.stepOff")
      : step.kind === "on"
        ? t("disc.stepOn")
        : `${t("disc.stepBit")} ${(step.bit ?? 0) + 1}`;

  return (
    <Card
      title={t("disc.title")}
      subtitle={t("disc.body")}
      actions={
        phase !== "idle" ? (
          <button className="btn btn-sm btn-ghost" onClick={cancel}>
            {t("common.cancel")}
          </button>
        ) : null
      }
    >
      {error && (
        <div style={{ marginBottom: "var(--s-4)" }}>
          <Alert tone="bad" icon={<Warning size={15} weight="fill" />}>
            {error}
          </Alert>
        </div>
      )}

      {phase === "idle" && (
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <ol
            className="hint"
            style={{ margin: 0, paddingLeft: "1.2em", lineHeight: 1.8 }}
          >
            <li>{t("disc.how1")}</li>
            <li>{t("disc.how2")}</li>
            <li>{t("disc.how3")}</li>
            <li>{t("disc.how4")}</li>
          </ol>
          <div className="row">
            <button
              className="btn btn-primary"
              onClick={start}
              disabled={ledCount === 0}
            >
              <Camera size={15} weight="fill" />
              {t("disc.start")}
            </button>
            {ledCount === 0 && <span className="hint">{t("disc.needLayout")}</span>}
          </div>
        </div>
      )}

      {phase === "shooting" && step && (
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <h2 style={{ fontSize: 17 }}>{stepLabel}</h2>
            <Badge>
              {current + 1} / {steps.length}
            </Badge>
          </div>
          <p className="hint">{t("disc.shoot")}</p>
          <div
            aria-hidden
            style={{
              height: 6,
              borderRadius: "var(--r-pill)",
              background: "var(--surface-3)",
              overflow: "hidden",
            }}
          >
            <div
              style={{
                width: `${((current + 1) / steps.length) * 100}%`,
                height: "100%",
                background: "var(--accent)",
              }}
            />
          </div>
          <div className="row">
            <button className="btn btn-primary" onClick={next}>
              {current + 1 >= steps.length ? t("disc.finishShots") : t("common.next")}
            </button>
          </div>
        </div>
      )}

      {phase === "upload" && (
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <Field label={t("disc.upload")} hint={t("disc.uploadHint")}>
            <input
              type="file"
              accept="image/*"
              multiple
              onChange={(e) => load(e.target.files)}
            />
          </Field>
          {busy && <span className="hint">{t("common.loading")}</span>}
        </div>
      )}

      {phase === "corners" && photos[1] && (
        <div style={{ display: "grid", gap: "var(--s-4)" }}>
          <p className="hint">
            {t("disc.corners")}{" "}
            <strong style={{ color: "var(--ink)" }}>
              {corners.length < 4 ? t(`disc.${CORNERS[corners.length]}`) : ""}
            </strong>
          </p>
          <div style={{ position: "relative", maxWidth: 640 }}>
            <img
              ref={imageRef}
              src={photos[1].url}
              alt={t("disc.corners")}
              onClick={markCorner}
              style={{
                width: "100%",
                borderRadius: "var(--r-md)",
                border: "1px solid var(--hairline)",
                cursor: corners.length < 4 ? "crosshair" : "default",
                display: "block",
              }}
            />
            {corners.map(([x, y], i) => (
              <span
                key={i}
                style={{
                  position: "absolute",
                  left: `${(x / photos[1].width) * 100}%`,
                  top: `${(y / photos[1].height) * 100}%`,
                  width: 12,
                  height: 12,
                  marginLeft: -6,
                  marginTop: -6,
                  borderRadius: "var(--r-pill)",
                  background: "var(--accent)",
                  border: "2px solid var(--canvas)",
                }}
              />
            ))}
          </div>
          <div className="row">
            <button
              className="btn btn-primary"
              onClick={build}
              disabled={corners.length !== 4 || busy}
            >
              {t("disc.build")}
            </button>
            <button className="btn btn-ghost" onClick={() => setCorners([])}>
              {t("common.reset")}
            </button>
          </div>
        </div>
      )}

      {phase === "done" && (
        <Alert icon={<CheckCircle size={15} weight="fill" />}>
          {t("disc.done")}
        </Alert>
      )}
    </Card>
  );
}
