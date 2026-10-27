import { useCallback, useEffect, useState } from "react";
import {
  getSettings,
  onModelStatus,
  onSettingsChanged,
  updateSettings,
  type SettingsData,
  type SettingsPatch,
} from "./api";

// Subscribes in an effect. The promise can resolve after unmount (StrictMode mounts twice), so a late
// unlisten is handled here once instead of in every caller.
export function useEvent(subscribe: () => Promise<() => void>) {
  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    subscribe().then((fn) => {
      if (disposed) fn();
      else off = fn;
    });
    return () => {
      disposed = true;
      off?.();
    };
    // The subscription is made once; `subscribe` is only read on mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

export interface SettingsStore {
  settings: SettingsData | null;
  update: (patch: SettingsPatch) => Promise<void>;
}

// The settings as Rust holds them. An edit shows at once and is replaced by the saved copy; a rejected
// edit reloads, so the screen never keeps a value that was not stored.
export function useSettings(): SettingsStore {
  const [settings, setSettings] = useState<SettingsData | null>(null);

  useEffect(() => {
    let alive = true;
    getSettings().then((s) => alive && setSettings(s));
    return () => {
      alive = false;
    };
  }, []);

  useEvent(() => onSettingsChanged(setSettings));
  useEvent(() => onModelStatus((model) => setSettings((s) => (s ? { ...s, model } : s))));

  const update = useCallback(async (patch: SettingsPatch) => {
    // The key is shown only after Rust has accepted it: the patch holds an id, the screen needs a label.
    const visible: Partial<SettingsData> = { ...patch, hotkey: undefined };
    delete visible.hotkey;
    setSettings((s) => (s ? { ...s, ...visible } : s));
    try {
      setSettings(await updateSettings(patch));
    } catch (e) {
      console.warn("update_settings failed", e);
      setSettings(await getSettings());
    }
  }, []);

  return { settings, update };
}

export interface PaneProps {
  settings: SettingsData;
  update: SettingsStore["update"];
}
