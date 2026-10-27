// Typed wrappers over the Tauri commands and events of the Settings and tour windows. The shapes mirror
// src-tauri/src/commands.rs. In the dev server (a plain browser) a mock backend answers instead; it is
// imported only when `import.meta.env.DEV` is true, so the production bundle has none of it.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { WidgetState, WidgetStyle } from "../widgetState";

export interface HotkeyInfo {
  id: string;
  label: string;
  isDefault: boolean;
}

export type ModelState = "downloading" | "loading" | "ready" | "failed";
export interface ModelInfo {
  state: ModelState;
  // 0 to 100 while downloading, null otherwise.
  percent: number | null;
}

export interface SettingsData {
  widgetStyle: WidgetStyle;
  glassOpacity: number;
  fadeWhenIdle: boolean;
  widgetVisible: boolean;
  hotkey: HotkeyInfo;
  model: ModelInfo;
  version: string;
}

// The strict patch: `hotkey` is a key id such as "rightCtrl".
export interface SettingsPatch {
  widgetStyle?: WidgetStyle;
  glassOpacity?: number;
  fadeWhenIdle?: boolean;
  widgetVisible?: boolean;
  hotkey?: string;
}

export type MicResult = "ok" | "silent" | "denied" | "noDevice" | "failed";
export interface MicCheck {
  result: MicResult;
  detail: string | null;
}

export interface PickEvent {
  result: "picked" | "cancelled" | "timedOut";
  hotkey: HotkeyInfo | null;
}

export interface DemoStep {
  state: WidgetState;
  seconds: number;
}

export interface Backend {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<() => void>;
}

const tauriBackend: Backend = {
  invoke: (command, args) => invoke(command, args),
  listen: (event, handler) => listen<never>(event, (e) => handler(e.payload as never)),
};

let backend: Backend = tauriBackend;

export const inTauri = () => "__TAURI_INTERNALS__" in window;

// Dev only: the page runs in a plain browser, so the commands have nothing to talk to.
export async function installDevBackendIfNeeded(): Promise<void> {
  if (!import.meta.env.DEV || inTauri()) return;
  const { createMockBackend } = await import("./mockBackend");
  backend = createMockBackend();
}

export const getSettings = () => backend.invoke<SettingsData>("get_settings");
export const updateSettings = (patch: SettingsPatch) => backend.invoke<SettingsData>("update_settings", { patch });
export const startHotkeyPick = () => backend.invoke<void>("start_hotkey_pick");
export const cancelHotkeyPick = () => backend.invoke<void>("cancel_hotkey_pick");
export const checkMicrophone = () => backend.invoke<MicCheck>("check_microphone");
export const openMicrophonePrivacy = () => backend.invoke<void>("open_microphone_privacy");
export const resetWidgetPosition = () => backend.invoke<void>("reset_widget_position");
export const widgetDemo = (style: WidgetStyle) => backend.invoke<DemoStep[]>("widget_demo", { style });
export const finishTour = () => backend.invoke<void>("finish_tour");
export const showTour = () => backend.invoke<void>("show_tour");

export const onSettingsChanged = (fn: (s: SettingsData) => void) => backend.listen<SettingsData>("settings-changed", fn);
export const onModelStatus = (fn: (m: ModelInfo) => void) => backend.listen<ModelInfo>("model-status", fn);
export const onHotkeyPick = (fn: (p: PickEvent) => void) => backend.listen<PickEvent>("hotkey-pick", fn);
