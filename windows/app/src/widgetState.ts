// Mirrors WidgetState in src-tauri/src/widget.rs (the `widget-state` event payload). Rust owns every
// size: `shape`, `caption` and `window` come from the core layout table and are used as they are.
export type WidgetPhase = "idle" | "listening" | "processing" | "done" | "failed";
export type WidgetStage = "loading" | "transcribing" | "formatting" | "inserting";
export type WidgetStyle = "flowPill" | "captionBar" | "island";

export interface WidgetState {
  phase: WidgetPhase;
  handsFree: boolean;
  message: string;
  // 32 entries, oldest first, each 0..1.
  levels: number[];
  // Set while processing; null otherwise.
  stage: WidgetStage | null;
  // Download fraction 0..1 while a dictation waits for the model; null otherwise.
  progress: number | null;
  style: WidgetStyle;
  glassOpacity: number;
  fadeWhenIdle: boolean;
  keyLabel: string;
  hovering: boolean;
  // The app the text goes to; null until the context is known.
  target: string | null;
  // Empty until the live preview exists (task 12).
  live: { stable: string; unstable: string };
  // Logical pixels.
  shape: { width: number; height: number; radius: number };
  // True when the flow pill shows its caption row above the capsule.
  caption: boolean;
  window: { width: number; height: number };
}

export const WIDGET_EVENT = "widget-state";

// Space around the shape for its shadow (WidgetLayout.margin). The window is the shape plus this on each
// side, so the shape is pinned to the bottom centre with exactly this much padding.
export const MARGIN = { x: 28, y: 26 };
