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

export type Polishing = "claude" | "codex" | "off";
export type ClaudeModel = "haiku" | "sonnet";
export type DictationMode = "raw" | "clean" | "prompt";

export interface SettingsData {
  widgetStyle: WidgetStyle;
  glassOpacity: number;
  fadeWhenIdle: boolean;
  widgetVisible: boolean;
  hotkey: HotkeyInfo;
  model: ModelInfo;
  version: string;
  // Polishing (docs/spec/settings.md). The dictionary is read-only here and not part of the payload.
  polishing: Polishing;
  claudeModel: ClaudeModel;
  // Empty means "found automatically".
  claudeExecutable: string;
  codexExecutable: string;
  defaultMode: DictationMode;
  // Rust clamps these to 0-30 and 10-200.
  fastPathMaxWords: number;
  promptModeMinWords: number;
  speakerNotes: string;
  // The daily update check (docs/spec/settings.md). The skipped version and the last check are not exposed.
  checkForUpdates: boolean;
}

// What `detect_agents` returns: the resolved path of each agent that was found (null when missing) and the
// settings after the fallback rule (a chosen agent that is gone falls back to the other one, or to off).
export interface AgentsInfo {
  claude: string | null;
  codex: string | null;
  settings: SettingsData;
}

// The strict patch: `hotkey` is a key id such as "rightCtrl".
export interface SettingsPatch {
  widgetStyle?: WidgetStyle;
  glassOpacity?: number;
  fadeWhenIdle?: boolean;
  widgetVisible?: boolean;
  hotkey?: string;
  polishing?: Polishing;
  claudeModel?: ClaudeModel;
  claudeExecutable?: string;
  codexExecutable?: string;
  defaultMode?: DictationMode;
  fastPathMaxWords?: number;
  promptModeMinWords?: number;
  speakerNotes?: string;
  checkForUpdates?: boolean;
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

export type UpdateStateId =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "installing"
  | "failed"
  | "installFailed";

// What `get_update_status` and the `update-status` event carry (src-tauri/src/updater.rs StatusPayload).
export interface UpdateStatus {
  state: UpdateStateId;
  label: string;
  // The error text for failed and installFailed, else null.
  message: string | null;
  // Unix seconds of the last check, null when there was none.
  lastCheck: number | null;
}

export interface CardButton {
  id: string;
  label: string;
  kind: "quiet" | "primary";
  // Sits at the left edge, the rest at the right.
  lead: boolean;
}

// The update card, computed in Rust (CardPayload). A null fraction draws an indeterminate bar.
export interface CardPayload {
  title: string;
  detail: string | null;
  progress: { fraction: number | null } | null;
  link: CardButton | null;
  buttons: CardButton[];
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
export const detectAgents = () => backend.invoke<AgentsInfo>("detect_agents");
export const finishTour = () => backend.invoke<void>("finish_tour");
export const showTour = () => backend.invoke<void>("show_tour");

export const getUpdateStatus = () => backend.invoke<UpdateStatus>("get_update_status");
export const checkUpdatesNow = () => backend.invoke<void>("check_updates_now");
export const showUpdateCard = () => backend.invoke<void>("show_update_card");
// The update window only. Returns the card to draw first; later ones arrive as `update-card`.
export const updateCardReady = () => backend.invoke<CardPayload>("update_card_ready");
export const updateAction = (action: string) => backend.invoke<void>("update_action", { action });
// Logical px: the card height plus its 20 px margins.
export const updateResize = (height: number) => backend.invoke<void>("update_resize", { height });

export const onSettingsChanged = (fn: (s: SettingsData) => void) => backend.listen<SettingsData>("settings-changed", fn);
export const onModelStatus = (fn: (m: ModelInfo) => void) => backend.listen<ModelInfo>("model-status", fn);
export const onHotkeyPick = (fn: (p: PickEvent) => void) => backend.listen<PickEvent>("hotkey-pick", fn);
export const onUpdateStatus = (fn: (s: UpdateStatus) => void) => backend.listen<UpdateStatus>("update-status", fn);
export const onUpdateCard = (fn: (c: CardPayload) => void) => backend.listen<CardPayload>("update-card", fn);
