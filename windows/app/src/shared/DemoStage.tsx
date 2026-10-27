import { useLayoutEffect, useRef, useState } from "react";
import { SPRING, SPRING_MS } from "../spring";
import WidgetView from "../WidgetView";
import type { WidgetState } from "../widgetState";
import type { DemoStep } from "./api";

const SIDE_PAD = 24;
const TOP_PAD = 28;
const BOTTOM_GAP = 14;

// A slice of desktop with the widget at the bottom, shrunk as a whole so the widest and tallest step
// fits, never above `maxScale` (DemoStage in WidgetDemo.swift).
export default function DemoStage({
  steps,
  state,
  elapsed,
  height,
  maxScale = 0.9,
}: {
  steps: DemoStep[];
  state: WidgetState | null;
  elapsed: string;
  height: number;
  maxScale?: number;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const widest = Math.max(1, ...steps.map((s) => s.state.window.width));
  const tallest = Math.max(1, ...steps.map((s) => s.state.window.height));
  const scale = Math.min(maxScale, (width - SIDE_PAD) / widest, (height - TOP_PAD) / tallest);

  return (
    <div ref={box} className="sp-demo" style={{ height }} aria-hidden="true">
      {state && width > 0 && scale > 0 && (
        <div
          className="sp-demo-inner"
          style={{ width: width / scale, height: height / scale, transform: `scale(${scale})`, pointerEvents: "none" }}
        >
          <div
            className="stage"
            style={
              {
                "--spring": SPRING,
                "--spring-ms": `${SPRING_MS}ms`,
                paddingBottom: BOTTOM_GAP / scale,
              } as React.CSSProperties
            }
          >
            <WidgetView state={state} elapsed={elapsed} />
          </div>
        </div>
      )}
    </div>
  );
}
