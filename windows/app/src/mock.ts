// Dev only (see main.tsx). Builds widget states from the layout fixture so every style and phase can be
// rendered in a plain browser: ?mock=<style>-<state>[&live=1&long=1&opacity=0.7&bg=light|dark]
import fixture from "../../core/tests/fixtures/widget-layout-windows.json";
import type { MockConfig } from "./Widget";
import type { WidgetPhase, WidgetStage, WidgetState, WidgetStyle } from "./widgetState";

interface Case {
  name: string;
  input: { style: WidgetStyle; phase: string; handsFree: boolean; hovering: boolean; hasLive: boolean };
  expected: {
    shape: { width: number; height: number; radius: number };
    caption: boolean;
    window: { width: number; height: number };
  };
}

const cases = fixture.cases as Case[];

const LIVE = { stable: "so the thing I wanted to say is that the", unstable: "widget should" };
const LONG_LIVE = {
  stable:
    "this is a much longer sentence that keeps going well past the width of the caption so that only the newest words stay in view",
  unstable: "right now",
};
const MESSAGES: Record<string, string> = {
  done: "Copied (press Ctrl+V to paste)",
  failed: "No microphone found",
};
const LONG_MESSAGE =
  "Could not paste into the focused app because it did not accept the text, so it was copied instead";

function levels(): number[] {
  return Array.from({ length: 32 }, (_, i) => 0.15 + 0.7 * Math.abs(Math.sin(i * 0.6) * Math.cos(i * 0.23)));
}

export function mockFromLocation(): MockConfig | undefined {
  const q = new URLSearchParams(window.location.search);
  const key = q.get("mock");
  if (!key) return undefined;

  const wantsLive = q.get("live") === "1";
  const base = key.replace("-", "/");
  // Live text on a plain recording changes the layout, so it maps to the fixture's own live case.
  const name = wantsLive && /\/(ptt|hands-free)$/.test(base) ? base.replace(/\/[^/]+$/, "/listening-live") : base;
  const c = cases.find((x) => x.name === name);
  if (!c) throw new Error(`no fixture case ${name}`);

  const phase = c.input.phase as WidgetPhase;
  const long = q.get("long") === "1";
  const live = phase === "idle" ? { stable: "", unstable: "" } : wantsLive || c.input.hasLive ? (long ? LONG_LIVE : LIVE) : { stable: "", unstable: "" };
  const stage: WidgetStage | null = phase === "processing" ? "transcribing" : null;
  const bg = q.get("bg");
  if (bg === "light" || bg === "dark") document.body.style.background = bg === "light" ? "#e9e9e9" : "#1c1c1c";
  const opacity = Number(q.get("opacity"));

  const state: WidgetState = {
    phase,
    handsFree: c.input.handsFree,
    message: long ? LONG_MESSAGE : (MESSAGES[phase] ?? ""),
    levels: phase === "listening" || phase === "processing" ? levels() : new Array(32).fill(0),
    stage,
    progress: null,
    style: c.input.style,
    glassOpacity: q.has("opacity") && Number.isFinite(opacity) ? opacity : 0.7,
    fadeWhenIdle: false,
    keyLabel: "Right Ctrl",
    hovering: c.input.hovering,
    target: phase === "idle" ? null : "Notepad",
    live,
    shape: c.expected.shape,
    caption: c.expected.caption,
    window: c.expected.window,
  };
  return { state, elapsedSeconds: 7 };
}
