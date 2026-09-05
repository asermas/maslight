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
  //
  // It stops while the window is hidden, which is most of the time. MasLight
  // lives in the tray: closing the window hides it and the engine keeps
  // driving the lights. An interval that kept running would ask the engine for
  // a frame five times a second and re-render a strip preview nobody can see,
  // for as long as the application is open. Hiding the window fires
  // visibilitychange, so there is no polling to notice that nobody is looking.
  //
  // Coming back ticks immediately rather than waiting out an interval, so the
  // window is never showing a stale number when it appears.
  useEffect(() => {
    let alive = true;
    let id = 0;

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

    const start = () => {
      if (id) return;
      tick();
      id = window.setInterval(tick, STATUS_INTERVAL_MS);
    };
    const stop = () => {
      if (!id) return;
      window.clearInterval(id);
      id = 0;
    };
    const follow = () => (document.hidden ? stop() : start());

    follow();
    document.addEventListener("visibilitychange", follow);
    return () => {
      alive = false;
      stop();
      document.removeEventListener("visibilitychange", follow);
    };
  }, []);

  // A rule can switch the profile behind the interface. Follow it locally so
  // the header matches what is actually running, but do not write it: the
  // profile someone picked by hand is still the one that comes back.
  useEffect(() => {
    if (!status?.profileId) return;
    setConfig((current) => {
      if (!current || current.activeProfile === status.profileId) return current;
      if (!current.profiles.some((p) => p.id === status.profileId)) return current;
      return { ...current, activeProfile: status.profileId };
    });
  }, [status?.profileId]);

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
