// Mirrors WidgetState in src-tauri/src/widget.rs (the `widget-state` event payload).
export type WidgetPhase = "idle" | "listening" | "processing" | "done" | "failed";
export type WidgetStage = "loading" | "transcribing" | "inserting";

export interface WidgetState {
  phase: WidgetPhase;
  handsFree: boolean;
  message: string;
  levels: number[];
  // Set while processing; null otherwise.
  stage: WidgetStage | null;
  // Download fraction 0..1 while a dictation waits for the model; null otherwise.
  progress: number | null;
}

export const WIDGET_EVENT = "widget-state";
export const BAR_COUNT = 32;

export const IDLE_STATE: WidgetState = {
  phase: "idle",
  handsFree: false,
  message: "",
  levels: [],
  stage: null,
  progress: null,
};
