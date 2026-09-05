/** Small building blocks shared by every screen. */

import type { ReactNode } from "react";

export function Mark({ size = 26 }: { size?: number }) {
  // The brand mark, not an icon. Kept inline so it inherits the theme and
  // needs no network request.
  return (
    <svg viewBox="0 0 256 256" width={size} height={size} aria-label="MasLight">
      <path
        d="M56 188 V72 L128 142 L200 72 V188"
        fill="none"
        stroke="currentColor"
        strokeWidth="26"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="56" cy="220" r="12" fill="#FF4D4D" />
      <circle cx="128" cy="220" r="12" fill="#4DFF9E" />
      <circle cx="200" cy="220" r="12" fill="#4DA6FF" />
    </svg>
  );
}

export function Card({
  title,
  subtitle,
  actions,
  children,
  tight,
}: {
  title?: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  tight?: boolean;
}) {
  return (
    <section className={tight ? "card card-tight" : "card"}>
      {(title || actions) && (
        <header className="card-head">
          <div>
            {title && <h3>{title}</h3>}
            {subtitle && <p>{subtitle}</p>}
          </div>
          {actions}
        </header>
      )}
      {children}
    </section>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
      {hint && <span className="hint">{hint}</span>}
    </div>
  );
}

export function Slider({
  label,
  hint,
  value,
  min,
  max,
  step = 1,
  format,
  onChange,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  format?: (v: number) => string;
  onChange: (v: number) => void;
}) {
  return (
    <div className="field">
      <label>{label}</label>
      <div className="slider-row">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          aria-label={label}
          onChange={(e) => onChange(Number(e.target.value))}
        />
        <output>{format ? format(value) : value}</output>
      </div>
      {hint && <span className="hint">{hint}</span>}
    </div>
  );
}

export function Toggle({
  label,
  checked,
  onChange,
}: {
  label: ReactNode;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="toggle">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-track" />
      <span>{label}</span>
    </label>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented" role="group">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Badge({
  tone = "neutral",
  children,
}: {
  tone?: "neutral" | "ok" | "warn" | "bad";
  children: ReactNode;
}) {
  const cls =
    tone === "ok"
      ? "badge badge-ok"
      : tone === "warn"
        ? "badge badge-warn"
        : tone === "bad"
          ? "badge badge-bad"
          : "badge";
  return <span className={cls}>{children}</span>;
}

/**
 * Health of one output device.
 *
 * A UDP sink cannot know whether anything is listening, so "sending" and
 * "reachable" are two different claims and the badge only makes the one it
 * can support.
 */
export function DeviceBadge({
  connected,
  reachable,
  labels,
}: {
  connected: boolean;
  reachable: boolean | null;
  labels: { sending: string; noAnswer: string; off: string };
}) {
  if (!connected) {
    return (
      <Badge tone="bad">
        <span className="dot" />
        {labels.off}
      </Badge>
    );
  }
  if (reachable === false) {
    return (
      <Badge tone="warn">
        <span className="dot" />
        {labels.noAnswer}
      </Badge>
    );
  }
  return (
    <Badge tone="ok">
      <span className="dot" />
      {labels.sending}
    </Badge>
  );
}

export function Stat({ value, label }: { value: ReactNode; label: string }) {
  return (
    <div className="stat">
      <span className="stat-value">{value}</span>
      <span className="stat-label">{label}</span>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  body,
  action,
}: {
  icon?: ReactNode;
  title: string;
  body?: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      {icon}
      <h3>{title}</h3>
      {body && <p style={{ maxWidth: "46ch" }}>{body}</p>}
      {action}
    </div>
  );
}

export function Alert({
  tone = "neutral",
  icon,
  children,
}: {
  tone?: "neutral" | "bad";
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={tone === "bad" ? "alert alert-bad" : "alert"}>
      {icon}
      <div>{children}</div>
    </div>
  );
}

export function NumberInput({
  value,
  min,
  max,
  step = 1,
  onChange,
  ariaLabel,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (v: number) => void;
  ariaLabel?: string;
}) {
  return (
    <input
      type="number"
      value={Number.isFinite(value) ? value : 0}
      min={min}
      max={max}
      step={step}
      aria-label={ariaLabel}
      onChange={(e) => {
        const next = Number(e.target.value);
        if (Number.isFinite(next)) onChange(next);
      }}
    />
  );
}
