import { useEffect, useState } from "react";

type SegState = "waiting" | "active" | "done";

// The active segment fills to 92% over 1.4 s and waits for the next stage (StepSegment in WidgetView.swift).
function Segment({ state, label, height }: { state: SegState; label?: string; height: number }) {
  const [fill, setFill] = useState(0);
  useEffect(() => {
    if (state !== "active") return;
    const id = requestAnimationFrame(() => setFill(0.92));
    return () => cancelAnimationFrame(id);
  }, [state]);

  const pct = state === "done" ? 1 : state === "active" ? fill : 0;
  return (
    <div className={label ? "seg labelled" : "seg"} style={{ height }}>
      <div className="seg-fill" style={{ width: `${pct * 100}%` }} />
      {label && <span className={state === "waiting" ? "seg-label dim" : "seg-label"}>{label}</span>}
    </div>
  );
}

export default function StepBar({ step, labels, height = 2 }: { step: number; labels?: string[]; height?: number }) {
  return (
    <div className="steps">
      {[0, 1, 2].map((i) => (
        <Segment
          // A new active step restarts the fill, like the .id() in the macOS view.
          key={i === step ? `active-${step}` : `seg-${i}`}
          state={i < step ? "done" : i === step ? "active" : "waiting"}
          label={labels?.[i]}
          height={height}
        />
      ))}
    </div>
  );
}
