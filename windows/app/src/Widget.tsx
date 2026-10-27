import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CaptionBar from "./CaptionBar";
import { formatElapsed } from "./captionContent";
import FlowPill from "./FlowPill";
import Island from "./Island";
import { SPRING, SPRING_MS } from "./spring";
import { MARGIN, WIDGET_EVENT, type WidgetState } from "./widgetState";

// Commands only exist inside the Tauri webview; the dev page runs in a plain browser.
const inTauri = () => "__TAURI_INTERNALS__" in window;
function send(command: string, args?: Record<string, unknown>) {
  if (!inTauri()) return;
  invoke(command, args).catch((e: unknown) => console.warn(`${command} failed`, e));
}

const FADE_AFTER_MS = 3000;
const FADED_OPACITY = 0.55;
// If the window has not grown to the new size by now, show the state anyway.
const GROW_FALLBACK_MS = 100;

interface View {
  state: WidgetState;
  // When the current recording started, in Date.now() time.
  startedAt: number;
}

export interface MockConfig {
  state: WidgetState;
  // Pretend the recording started this many seconds ago.
  elapsedSeconds: number;
}

function covers(w: { width: number; height: number }): boolean {
  return window.innerWidth >= w.width - 0.5 && window.innerHeight >= w.height - 0.5;
}

export default function Widget({ preset }: { preset?: MockConfig }) {
  const [view, setView] = useState<View | null>(() =>
    preset ? { state: preset.state, startedAt: Date.now() - preset.elapsedSeconds * 1000 } : null,
  );
  const viewRef = useRef<View | null>(view);
  const pending = useRef<{ cancel: () => void } | null>(null);

  const apply = useCallback((next: WidgetState) => {
    const prev = viewRef.current;
    const continuing = prev?.state.phase === "listening" && next.phase === "listening";
    const v: View = { state: next, startedAt: continuing ? prev.startedAt : Date.now() };
    viewRef.current = v;
    setView(v);
  }, []);

  // The window grows before the shape springs: hold a state until the webview is as big as the payload
  // says the window is. Shrinking needs no wait, the window is still large when the shape gets smaller.
  const receive = useCallback(
    (next: WidgetState) => {
      pending.current?.cancel();
      pending.current = null;
      if (covers(next.window)) {
        apply(next);
        return;
      }
      const done = () => {
        cancel();
        apply(next);
      };
      const onResize = () => {
        if (covers(next.window)) done();
      };
      const timer = window.setTimeout(done, GROW_FALLBACK_MS);
      const cancel = () => {
        window.clearTimeout(timer);
        window.removeEventListener("resize", onResize);
        pending.current = null;
      };
      window.addEventListener("resize", onResize);
      pending.current = { cancel };
    },
    [apply],
  );

  useEffect(() => {
    if (preset || !inTauri()) return;
    // The promise resolves after unmount in StrictMode, so track that and unlisten late.
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<WidgetState>(WIDGET_EVENT, (e) => receive(e.payload)).then((fn) => {
      if (disposed) {
        fn();
        return;
      }
      unlisten = fn;
      // Rust emits the current state once we are listening.
      send("widget_ready");
    });
    return () => {
      disposed = true;
      unlisten?.();
      pending.current?.cancel();
    };
  }, [preset, receive]);

  const state = view?.state ?? null;
  const listening = state?.phase === "listening";

  // Recording timer, m:ss.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!listening) return;
    const id = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(id);
  }, [listening]);

  // Fade after 3 s idle without hover. The flag is only set by the timer and cleared when it is torn
  // down, so it can never outlive the conditions that started it.
  const [faded, setFaded] = useState(false);
  const canFade = Boolean(state && state.phase === "idle" && !state.hovering && state.fadeWhenIdle);
  useEffect(() => {
    if (!canFade) return;
    const id = setTimeout(() => setFaded(true), FADE_AFTER_MS);
    return () => {
      clearTimeout(id);
      setFaded(false);
    };
  }, [canFade]);

  const dragging = useRef(false);
  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    // Capture keeps move and up coming when the cursor leaves the window mid-drag or after a hold.
    e.currentTarget.setPointerCapture(e.pointerId);
    dragging.current = true;
    send("widget_pointer", { kind: "down" });
  };
  const onPointerMove = () => {
    if (dragging.current) send("widget_pointer", { kind: "move" });
  };
  const onPointerUp = () => {
    if (!dragging.current) return;
    dragging.current = false;
    send("widget_pointer", { kind: "up" });
  };

  if (!state || !view) return null;

  const props = { s: state, elapsed: formatElapsed(now - view.startedAt) };
  const opacity = faded ? FADED_OPACITY : 1;
  // const opacity = faded ? FADED_OPACITY : 1;
  return (
    <div
      className="stage"
      style={
        {
          "--spring": SPRING,
          "--spring-ms": `${SPRING_MS}ms`,
          padding: `${MARGIN.y}px ${MARGIN.x}px`,
          opacity,
          // The dev page has no native window: give the stage the exact window size, so a
          // viewport wider than the window (headless Chrome has a minimum width) cannot hide a layout bug.
          ...(preset
            ? { position: "absolute", left: 0, bottom: 0, width: state.window.width, height: state.window.height }
            : {}),
        } as React.CSSProperties
      }
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onMouseEnter={() => send("widget_hover", { hovering: true })}
      onMouseLeave={() => send("widget_hover", { hovering: false })}
      onContextMenu={(e) => e.preventDefault()}
    >
      {state.style === "captionBar" ? (
        <CaptionBar {...props} />
      ) : state.style === "island" ? (
        <Island {...props} />
      ) : (
        <FlowPill {...props} />
      )}
    </div>
  );
}
