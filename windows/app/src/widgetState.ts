// Mirrors WidgetState in src-tauri/src/widget.rs (the `widget-state` event payload).
export type WidgetPhase = "idle" | "listening" | "failed";

export interface WidgetState {
  phase: WidgetPhase;
  handsFree: boolean;
  message: string;
  levels: number[];
}

export const WIDGET_EVENT = "widget-state";
export const BAR_COUNT = 32;

export const IDLE_STATE: WidgetState = {
  phase: "idle",
  handsFree: false,
  message: "",
  levels: [],
};
