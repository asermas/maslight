/**
 * Application state.
 *
 * One hook owns the configuration, the engine status poll and the save queue.
 * Screens read from it and call `update`; nothing else talks to the backend.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { api, errorText } from "./api";
import { resolveLang, translator, type Lang } from "./i18n";
import type { AppConfig, AppInfo, EngineStatus, Profile } from "./types";
import { activeProfile, withActiveProfile } from "./types";

/** How often the dashboard asks the engine how it is doing. */
const STATUS_INTERVAL_MS = 200;
/** Settings changes are coalesced so dragging a slider is not a write storm. */
const SAVE_DEBOUNCE_MS = 250;

export interface Store {
  config: AppConfig | null;
  status: EngineStatus | null;
  info: AppInfo | null;
  profile: Profile | null;
  lang: Lang;
  t: ReturnType<typeof translator>;
  loading: boolean;
  error: string | null;
  notice: string | null;
  setNotice: (message: string | null) => void;
  update: (next: AppConfig) => void;
  updateProfile: (update: (profile: Profile) => Profile) => void;
  setEnabled: (enabled: boolean) => void;
  reload: () => void;
}

export function useMasLight(): Store {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [status, setStatus] = useState<EngineStatus | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const pending = useRef<AppConfig | null>(null);
  const timer = useRef<number | null>(null);

  const reload = useCallback(() => {
    setLoading(true);
    Promise.all([api.getConfig(), api.appInfo()])
      .then(([c, i]) => {
        setConfig(c);
        setInfo(i);
        setError(null);
      })
      .catch((e) => setError(errorText(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(reload, [reload]);

  // Status poll. A plain interval is right here: the payload is small and the
  // engine owns its own clock, so there is nothing to synchronise.
  useEffect(() => {
    let alive = true;
    const tick = () => {
      api
        .getStatus()
        .then((s) => {
          if (alive) setStatus(s);
        })
        .catch(() => {
          /* the engine restarts on its own; a missed poll is not an error */
        });
    };
    tick();
    const id = window.setInterval(tick, STATUS_INTERVAL_MS);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  const flush = useCallback(() => {
    const next = pending.current;
    pending.current = null;
    timer.current = null;
    if (!next) return;
    api
      .saveConfig(next)
      .then((saved) => {
        setConfig(saved);
        setError(null);
      })
      .catch((e) => setError(errorText(e)));
  }, []);

  const update = useCallback(
    (next: AppConfig) => {
      setConfig(next);
      pending.current = next;
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = window.setTimeout(flush, SAVE_DEBOUNCE_MS);
    },
    [flush]
  );

  // Never lose an edit because the window closed mid-debounce.
  useEffect(() => {
    const onHide = () => {
      if (timer.current !== null) {
        window.clearTimeout(timer.current);
        flush();
      }
    };
    window.addEventListener("beforeunload", onHide);
    return () => window.removeEventListener("beforeunload", onHide);
  }, [flush]);

  const updateProfile = useCallback(
    (fn: (profile: Profile) => Profile) => {
      setConfig((current) => {
        if (!current) return current;
        const next = withActiveProfile(current, fn);
        pending.current = next;
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = window.setTimeout(flush, SAVE_DEBOUNCE_MS);
        return next;
      });
    },
    [flush]
  );

  const setEnabled = useCallback(
    (enabled: boolean) => {
      setConfig((current) => (current ? { ...current, enabled } : current));
      api.setEnabled(enabled).catch((e) => setError(errorText(e)));
    },
    []
  );

  const lang = useMemo(
    () => resolveLang(config?.ui.language ?? "system"),
    [config?.ui.language]
  );
  const t = useMemo(() => translator(lang), [lang]);
  const profile = config ? activeProfile(config) : null;

  // Theme is applied on the document so CSS custom properties can swap.
  useEffect(() => {
    const preference = config?.ui.theme ?? "dark";
    const root = document.documentElement;
    const apply = () => {
      const dark =
        preference === "dark" ||
        (preference === "system" &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      root.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    if (preference !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [config?.ui.theme]);

  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  // Notices fade on their own so they never pile up.
  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 2600);
    return () => window.clearTimeout(id);
  }, [notice]);

  return {
    config,
    status,
    info,
    profile,
    lang,
    t,
    loading,
    error,
    notice,
    setNotice,
    update,
    updateProfile,
    setEnabled,
    reload,
  };
}
