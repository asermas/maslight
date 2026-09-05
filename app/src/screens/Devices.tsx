import { useState } from "react";
import { MagnifyingGlass, Plugs, Trash } from "@phosphor-icons/react";

import { Badge, Card, EmptyState, Field, NumberInput } from "../components/ui";
import { api, errorText } from "../lib/api";
import type { Store } from "../lib/store";
import type {
  DeviceConfig,
  DiscoveredDevice,
  WledProtocol,
} from "../lib/types";
import { deviceLabel } from "../lib/types";

type Kind = DeviceConfig["kind"];

const DEFAULTS: Record<Kind, DeviceConfig> = {
  wled: { kind: "wled", host: "", port: 21324, protocol: "dnrgb", timeoutS: 2 },
  ddp: { kind: "ddp", host: "", port: 4048, startOffset: 0 },
  e131: { kind: "e131", host: null, universe: 1, priority: 100 },
  "art-net": {
    kind: "art-net",
    host: "",
    port: 6454,
    universe: 0,
    net: 0,
    subnet: 0,
  },
  serial: { kind: "serial", port: "", baud: 115200, protocol: "adalight" },
  null: { kind: "null" },
};

export function Devices({ store }: { store: Store }) {
  const { t, profile, updateProfile, status, setNotice } = store;
  const [scanning, setScanning] = useState(false);
  const [found, setFound] = useState<DiscoveredDevice[] | null>(null);
  const [draft, setDraft] = useState<DeviceConfig>(DEFAULTS.wled);
  const [error, setError] = useState<string | null>(null);

  if (!profile) return null;

  const scan = async () => {
    setScanning(true);
    setError(null);
    try {
      setFound(await api.discoverDevices(3000));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setScanning(false);
    }
  };

  const attach = (device: DeviceConfig) => {
    updateProfile((p) => ({ ...p, devices: [...p.devices, device] }));
    setNotice(deviceLabel(device));
  };

  const remove = (index: number) =>
    updateProfile((p) => ({
      ...p,
      devices: p.devices.filter((_, i) => i !== index),
    }));

  const useDiscovered = (d: DiscoveredDevice) => {
    const count = d.ledCount ?? 0;
    attach({
      kind: "wled",
      host: d.host,
      port: 21324,
      protocol: count > 490 ? "dnrgb" : "drgb",
      timeoutS: 2,
    });
  };

  const probe = async () => {
    if (draft.kind !== "wled" || !draft.host) return;
    setError(null);
    const info = await api.probeDevice(draft.host).catch((e) => {
      setError(errorText(e));
      return null;
    });
    if (info) {
      setFound([info]);
      setNotice(
        info.ledCount ? `${info.name}: ${info.ledCount} LED` : info.name
      );
    } else {
      setError(t("devices.notFound"));
    }
  };

  return (
    <div className="stack">
      <Card
        title={t("devices.attached")}
        actions={
          <button className="btn btn-sm" onClick={scan} disabled={scanning}>
            <MagnifyingGlass size={14} weight="bold" />
            {scanning ? t("devices.scanning") : t("devices.scan")}
          </button>
        }
      >
        {profile.devices.length === 0 ? (
          <EmptyState
            icon={<Plugs size={26} />}
            title={t("devices.none")}
            body={t("devices.noneBody")}
          />
        ) : (
          <div className="list">
            {profile.devices.map((d, i) => {
              const live = status?.devices?.[i];
              return (
                <div className="list-row" key={i}>
                  <div className="list-row-main">
                    <strong>{deviceLabel(d)}</strong>
                    <div>
                      {d.kind === "wled"
                        ? d.protocol.toUpperCase()
                        : d.kind.toUpperCase()}
                      {live?.error ? ` - ${live.error}` : ""}
                    </div>
                  </div>
                  {live && (
                    <Badge tone={live.connected ? "ok" : "bad"}>
                      <span className="dot" />
                      {live.connected ? t("common.on") : t("common.off")}
                    </Badge>
                  )}
                  <button
                    className="btn btn-sm btn-danger"
                    onClick={() => remove(i)}
                    aria-label={t("common.remove")}
                  >
                    <Trash size={14} />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      {found !== null && (
        <Card title={t("devices.found")}>
          {found.length === 0 ? (
            <EmptyState
              title={t("devices.notFound")}
              body={t("devices.notFoundBody")}
            />
          ) : (
            <div className="list">
              {found.map((d) => (
                <div className="list-row" key={d.host}>
                  <div className="list-row-main">
                    <strong>{d.name}</strong>
                    <div>
                      {d.host}
                      {d.ledCount
                        ? ` - ${d.ledCount} ${t("devices.ledCount")}`
                        : ""}
                      {d.version ? ` - ${d.version}` : ""}
                    </div>
                  </div>
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => useDiscovered(d)}
                  >
                    {t("devices.useIt")}
                  </button>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}

      <Card title={t("devices.manual")}>
        <div className="grid-2">
          <Field label={t("devices.type")}>
            <select
              value={draft.kind}
              onChange={(e) => setDraft(DEFAULTS[e.target.value as Kind])}
            >
              <option value="wled">WLED (UDP)</option>
              <option value="ddp">DDP</option>
              <option value="e131">sACN (E1.31)</option>
              <option value="art-net">Art-Net</option>
              <option value="serial">Adalight / TPM2</option>
            </select>
          </Field>

          {draft.kind === "serial" ? (
            <Field label={t("devices.serialPort")}>
              <input
                type="text"
                value={draft.port}
                placeholder="COM3"
                onChange={(e) => setDraft({ ...draft, port: e.target.value })}
              />
            </Field>
          ) : draft.kind === "e131" ? (
            <Field label={t("devices.host")} hint="multicast">
              <input
                type="text"
                value={draft.host ?? ""}
                placeholder="192.168.0.200"
                onChange={(e) =>
                  setDraft({ ...draft, host: e.target.value || null })
                }
              />
            </Field>
          ) : draft.kind === "wled" ||
            draft.kind === "ddp" ||
            draft.kind === "art-net" ? (
            <Field label={t("devices.host")}>
              <input
                type="text"
                value={draft.host}
                placeholder="192.168.0.200"
                onChange={(e) => setDraft({ ...draft, host: e.target.value })}
              />
            </Field>
          ) : null}

          {draft.kind === "wled" && (
            <>
              <Field label={t("devices.protocol")}>
                <select
                  value={draft.protocol}
                  onChange={(e) =>
                    setDraft({
                      ...draft,
                      protocol: e.target.value as WledProtocol,
                    })
                  }
                >
                  <option value="dnrgb">DNRGB</option>
                  <option value="drgb">DRGB</option>
                  <option value="warls">WARLS</option>
                  <option value="drgbw">DRGBW</option>
                  <option value="dnrgbw">DNRGBW</option>
                </select>
              </Field>
              <Field label={t("devices.port")}>
                <NumberInput
                  value={draft.port}
                  min={1}
                  max={65535}
                  onChange={(v) => setDraft({ ...draft, port: Math.round(v) })}
                />
              </Field>
              <Field label={t("devices.timeout")} hint={t("devices.timeoutHint")}>
                <NumberInput
                  value={draft.timeoutS}
                  min={1}
                  max={255}
                  onChange={(v) =>
                    setDraft({ ...draft, timeoutS: Math.round(v) })
                  }
                />
              </Field>
            </>
          )}

          {(draft.kind === "e131" || draft.kind === "art-net") && (
            <Field label={t("devices.universe")}>
              <NumberInput
                value={draft.universe}
                min={0}
                max={63999}
                onChange={(v) =>
                  setDraft({ ...draft, universe: Math.round(v) })
                }
              />
            </Field>
          )}

          {draft.kind === "serial" && (
            <Field label={t("devices.baud")}>
              <NumberInput
                value={draft.baud}
                min={9600}
                max={2000000}
                step={100}
                onChange={(v) => setDraft({ ...draft, baud: Math.round(v) })}
              />
            </Field>
          )}
        </div>

        {error && (
          <p className="hint" style={{ marginTop: "var(--s-3)", color: "var(--danger)" }}>
            {error}
          </p>
        )}

        <div className="row" style={{ marginTop: "var(--s-4)" }}>
          <button
            className="btn btn-primary"
            disabled={
              (draft.kind === "serial" && !draft.port) ||
              (draft.kind !== "serial" &&
                draft.kind !== "e131" &&
                draft.kind !== "null" &&
                !draft.host)
            }
            onClick={() => attach(draft)}
          >
            {t("common.add")}
          </button>
          {draft.kind === "wled" && (
            <button className="btn" onClick={probe} disabled={!draft.host}>
              {t("devices.probe")}
            </button>
          )}
        </div>
      </Card>
    </div>
  );
}
