import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { BAR_COUNT, IDLE_STATE, WIDGET_EVENT, type WidgetState } from "./widgetState";

// WidgetModel.swift:21-22 words the stages the same way.
function processingLabel({ stage, progress }: WidgetState): string {
  if (progress !== null) return `Downloading model ${Math.round(progress * 100)}%`;
  return stage === "loading" ? "Loading model" : "Transcribing";
}

export default function Widget() {
  const [state, setState] = useState<WidgetState>(IDLE_STATE);

  useEffect(() => {
    // The promise resolves after unmount in StrictMode, so track that and unlisten late.
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<WidgetState>(WIDGET_EVENT, (e) => setState(e.payload)).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const { phase, handsFree, message, levels } = state;
  const caption =
    phase === "processing"
      ? { text: processingLabel(state), dim: true }
      : (phase === "failed" || phase === "done") && message
        ? { text: message, dim: false }
        : null;
  const cls = ["pill", phase, phase === "listening" && handsFree ? "hands-free" : ""]
    .filter(Boolean)
    .join(" ");

  return (
    <>
      {caption && <div className={caption.dim ? "caption dim" : "caption"}>{caption.text}</div>}
      <div className={cls}>
        {phase === "listening" &&
          Array.from({ length: BAR_COUNT }, (_, i) => {
            const level = Math.min(1, Math.max(0, levels[i] ?? 0));
            return <span key={i} className="bar" style={{ height: 4 + 16 * level }} />;
          })}
      </div>
    </>
  );
}
