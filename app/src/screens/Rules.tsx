import { ArrowDown, ArrowUp, Plus, Trash, TreeStructure } from "@phosphor-icons/react";

import { Badge, Card, EmptyState, Field } from "../components/ui";
import type { Store } from "../lib/store";
import type { AutoRule } from "../lib/types";

type RuleKind = AutoRule["when"];

/** A fresh rule of the given kind, pointing at a profile that exists. */
function blankRule(kind: RuleKind, profile: string): AutoRule {
  switch (kind) {
    case "process-running":
      return { when: "process-running", process: "", profile };
    case "time-range":
      return {
        when: "time-range",
        fromMinutes: 23 * 60,
        toMinutes: 7 * 60,
        profile,
      };
    case "on-battery":
      return { when: "on-battery", profile };
    default:
      return { when: "fullscreen", profile };
  }
}

/** Minutes past midnight as `HH:MM`, and back. */
function toClock(minutes: number): string {
  const m = ((Math.round(minutes) % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

function fromClock(value: string): number {
  const [h, m] = value.split(":").map((v) => Number.parseInt(v, 10));
  if (Number.isNaN(h) || Number.isNaN(m)) return 0;
  return (h * 60 + m) % 1440;
}

export function Rules({ store }: { store: Store }) {
  const { t, config, status, update } = store;
  if (!config) return null;

  const rules = config.rules;
  const setRules = (next: AutoRule[]) => update({ ...config, rules: next });

  const patch = (index: number, rule: AutoRule) =>
    setRules(rules.map((r, i) => (i === index ? rule : r)));

  const move = (index: number, delta: number) => {
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    const next = [...rules];
    [next[index], next[target]] = [next[target], next[index]];
    setRules(next);
  };

  return (
    <div className="stack">
      <Card
        title={t("rules.title")}
        subtitle={t("rules.body")}
        actions={
          status ? (
            <div className="row" style={{ gap: "var(--s-2)" }}>
              <Badge tone={status.ruleFullscreen ? "ok" : "neutral"}>
                {t("rules.nowFullscreen")}
              </Badge>
              <Badge tone={status.ruleOnBattery ? "warn" : "neutral"}>
                {t("rules.nowBattery")}
              </Badge>
              <Badge>{toClock(status.ruleMinutes)}</Badge>
            </div>
          ) : null
        }
      >
        {rules.length === 0 ? (
          <EmptyState
            icon={<TreeStructure size={26} />}
            title={t("rules.empty")}
            body={t("rules.emptyBody")}
          />
        ) : (
          <div className="list">
            {rules.map((rule, i) => (
              <div className="list-row" key={i} style={{ alignItems: "flex-end" }}>
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "180px minmax(0, 1fr) 180px",
                    gap: "var(--s-3)",
                    flex: 1,
                  }}
                >
                  <Field label={t("rules.when")}>
                    <select
                      value={rule.when}
                      onChange={(e) =>
                        patch(
                          i,
                          blankRule(e.target.value as RuleKind, rule.profile)
                        )
                      }
                    >
                      <option value="process-running">
                        {t("rules.whenProcess")}
                      </option>
                      <option value="fullscreen">
                        {t("rules.whenFullscreen")}
                      </option>
                      <option value="time-range">{t("rules.whenTime")}</option>
                      <option value="on-battery">
                        {t("rules.whenBattery")}
                      </option>
                    </select>
                  </Field>

                  <div>
                    {rule.when === "process-running" && (
                      <Field label={t("rules.process")} hint={t("rules.processHint")}>
                        <input
                          type="text"
                          value={rule.process}
                          placeholder="witcher"
                          onChange={(e) =>
                            patch(i, { ...rule, process: e.target.value })
                          }
                        />
                      </Field>
                    )}
                    {rule.when === "time-range" && (
                      <div className="row" style={{ gap: "var(--s-2)" }}>
                        <Field label={t("rules.from")}>
                          <input
                            type="time"
                            value={toClock(rule.fromMinutes)}
                            onChange={(e) =>
                              patch(i, {
                                ...rule,
                                fromMinutes: fromClock(e.target.value),
                              })
                            }
                          />
                        </Field>
                        <Field label={t("rules.to")}>
                          <input
                            type="time"
                            value={toClock(rule.toMinutes)}
                            onChange={(e) =>
                              patch(i, {
                                ...rule,
                                toMinutes: fromClock(e.target.value),
                              })
                            }
                          />
                        </Field>
                      </div>
                    )}
                    {(rule.when === "fullscreen" ||
                      rule.when === "on-battery") && (
                      <span className="hint">
                        {rule.when === "fullscreen"
                          ? t("rules.fullscreenHint")
                          : t("rules.batteryHint")}
                      </span>
                    )}
                  </div>

                  <Field label={t("rules.then")}>
                    <select
                      value={rule.profile}
                      onChange={(e) =>
                        patch(i, { ...rule, profile: e.target.value })
                      }
                    >
                      {config.profiles.map((p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      ))}
                    </select>
                  </Field>
                </div>

                <div className="row" style={{ gap: 4 }}>
                  <button
                    className="btn btn-sm btn-icon"
                    disabled={i === 0}
                    aria-label={t("rules.moveUp")}
                    onClick={() => move(i, -1)}
                  >
                    <ArrowUp size={13} />
                  </button>
                  <button
                    className="btn btn-sm btn-icon"
                    disabled={i === rules.length - 1}
                    aria-label={t("rules.moveDown")}
                    onClick={() => move(i, 1)}
                  >
                    <ArrowDown size={13} />
                  </button>
                  <button
                    className="btn btn-sm btn-danger btn-icon"
                    aria-label={t("common.remove")}
                    onClick={() => setRules(rules.filter((_, j) => j !== i))}
                  >
                    <Trash size={13} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="row" style={{ marginTop: "var(--s-4)" }}>
          <button
            className="btn"
            onClick={() =>
              setRules([
                ...rules,
                blankRule("fullscreen", config.profiles[0].id),
              ])
            }
          >
            <Plus size={14} weight="bold" />
            {t("rules.add")}
          </button>
          {rules.length > 1 && (
            <span className="hint">{t("rules.orderHint")}</span>
          )}
        </div>
      </Card>
    </div>
  );
}
