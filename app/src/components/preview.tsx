/** Live strip preview and the layout canvas. */

import { useCallback, useRef, useState } from "react";

import type { LedSpec, Rgb8 } from "../lib/types";
import { rgbToCss } from "../lib/types";

/**
 * The colours currently on the wire, in chain order.
 *
 * This is the fastest way to see that the pipeline is alive: if the strip is
 * dark here, the problem is upstream of the network.
 */
export function StripPreview({ leds }: { leds: Rgb8[] }) {
  if (leds.length === 0) {
    return <div className="strip skeleton" style={{ opacity: 0.5 }} />;
  }
  return (
    <div className="strip" role="img" aria-label="Live LED colours">
      {leds.map((c, i) => (
        <span
          key={i}
          className="strip-cell"
          style={{ background: rgbToCss(c) }}
        />
      ))}
    </div>
  );
}

/**
 * The screen with its LEDs laid around it.
 *
 * LEDs can be selected and dragged. Positions are normalised, so the canvas
 * works at any size and the layout survives a resolution change.
 */
export function LayoutCanvas({
  leds,
  colors,
  selected,
  onSelect,
  onMove,
  readOnly,
}: {
  leds: LedSpec[];
  colors?: Rgb8[];
  selected?: number | null;
  onSelect?: (index: number) => void;
  onMove?: (index: number, x: number, y: number) => void;
  readOnly?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState<number | null>(null);

  const positionFromEvent = useCallback((clientX: number, clientY: number) => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return null;
    return {
      x: Math.min(1, Math.max(0, (clientX - box.left) / box.width)),
      y: Math.min(1, Math.max(0, (clientY - box.top) / box.height)),
    };
  }, []);

  return (
    <div
      ref={ref}
      className="monitor"
      onPointerMove={(e) => {
        if (dragging === null || !onMove) return;
        const p = positionFromEvent(e.clientX, e.clientY);
        if (p) onMove(dragging, p.x, p.y);
      }}
      onPointerUp={() => setDragging(null)}
      onPointerLeave={() => setDragging(null)}
    >
      <div className="monitor-screen">
        {leds.length === 0 ? "No LEDs" : null}
      </div>
      {leds.map((led) => {
        const colour = colors?.[led.index];
        const background = colour
          ? rgbToCss(colour)
          : led.enabled
            ? "var(--accent)"
            : "var(--hairline-strong)";
        return (
          <button
            key={led.index}
            type="button"
            className="led"
            data-selected={selected === led.index}
            data-disabled={!led.enabled}
            title={`LED ${led.index}`}
            aria-label={`LED ${led.index}`}
            style={{
              left: `${led.rect.x * 100}%`,
              top: `${led.rect.y * 100}%`,
              width: `${Math.max(led.rect.w, 0.012) * 100}%`,
              height: `${Math.max(led.rect.h, 0.02) * 100}%`,
              background,
              padding: 0,
              cursor: readOnly ? "default" : "grab",
            }}
            onPointerDown={(e) => {
              if (readOnly) return;
              e.preventDefault();
              onSelect?.(led.index);
              setDragging(led.index);
            }}
            onFocus={() => onSelect?.(led.index)}
          />
        );
      })}
    </div>
  );
}
