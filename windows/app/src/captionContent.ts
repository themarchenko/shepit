import type { WidgetStage, WidgetState } from "./widgetState";

export type Tone = "text" | "amber";

export interface CaptionContent {
  label?: string;
  stable: string;
  unstable: string;
  dim: boolean;
  tone: Tone;
  // Live and processing words keep the newest end; a result or an error keeps its start, so
  // "Copied (press Ctrl+V to paste)" stays readable.
  clip: "start" | "end";
}

// WidgetModel.Stage.label.
export function stageLabel(stage: WidgetStage | null): string {
  if (stage === "loading") return "Loading model";
  if (stage === "formatting") return "Formatting";
  return stage === "inserting" ? "Inserting" : "Transcribing";
}

export function processingLabel(s: Pick<WidgetState, "stage" | "progress">): string {
  if (s.progress !== null) return `Downloading model ${Math.round(s.progress * 100)}%`;
  return stageLabel(s.stage);
}

// Position among the three steps the user sees: loading and transcribing are the first, formatting the
// second, inserting the last.
export function stepOf(stage: WidgetStage | null): number {
  if (stage === "formatting") return 1;
  return stage === "inserting" ? 2 : 0;
}

export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function liveFull(s: WidgetState): string {
  const { stable, unstable } = s.live;
  return unstable ? (stable ? `${stable} ${unstable}` : unstable) : stable;
}

// WidgetView.swift CaptionContent.of
export function captionContent(s: WidgetState): CaptionContent | null {
  switch (s.phase) {
    case "idle":
      return null;
    case "listening":
      return s.live.stable || s.live.unstable
        ? { stable: s.live.stable, unstable: s.live.unstable, dim: false, tone: "text", clip: "start" }
        : null;
    case "processing":
      return {
        label: processingLabel(s),
        stable: liveFull(s),
        unstable: "",
        dim: true,
        tone: "text",
        clip: "start",
      };
    case "done":
      return s.message ? { stable: s.message, unstable: "", dim: false, tone: "text", clip: "end" } : null;
    case "failed":
      return s.message ? { stable: s.message, unstable: "", dim: false, tone: "amber", clip: "end" } : null;
  }
}
