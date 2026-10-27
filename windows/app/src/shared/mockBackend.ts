// Dev only (see installDevBackendIfNeeded): answers the commands in a plain browser so every Settings and
// tour state can be rendered and checked, and scripts a few clicks for states that need them. Not part of
// the production bundle. States: ?mock=settings-{general,general-downloading,general-picking,
// general-rightalt,general-mic-silent,appearance-<style>,about} or ?mock=tour-{1,2,2-denied,3,4,4-downloading}.
import fixture from "../../../core/tests/fixtures/widget-layout-windows.json";
import type { WidgetPhase, WidgetStage, WidgetState, WidgetStyle } from "../widgetState";
import type { Backend, DemoStep, HotkeyInfo, MicCheck, MicResult, ModelInfo, PickEvent, SettingsData, SettingsPatch } from "./api";

const HOTKEYS: Record<string, string> = {
  rightCtrl: "Right Ctrl",
  rightAlt: "Right Alt",
  ...Object.fromEntries(Array.from({ length: 12 }, (_, i) => [`f${13 + i}`, `F${13 + i}`])),
};

function hotkey(id: string): HotkeyInfo {
  return { id, label: HOTKEYS[id] ?? "Right Ctrl", isDefault: id === "rightCtrl" };
}

interface Case {
  name: string;
  input: { style: WidgetStyle; phase: string; hovering: boolean };
  expected: { shape: { width: number; height: number; radius: number }; caption: boolean; window: { width: number; height: number } };
}
const cases = fixture.cases as Case[];

const DEMO_TEXT = "Зустрінемось завтра о десятій, I'll send the notes.";

function demoSteps(style: WidgetStyle, key: HotkeyInfo): DemoStep[] {
  const make = (caseName: string, seconds: number, stage: WidgetStage | null, message = ""): DemoStep => {
    const c = cases.find((x) => x.name === `${style}/${caseName}`);
    if (!c) throw new Error(`no fixture case ${style}/${caseName}`);
    const phase = c.input.phase as WidgetPhase;
    const state: WidgetState = {
      phase,
      handsFree: false,
      message,
      levels: new Array(32).fill(0),
      stage,
      progress: null,
      style,
      glassOpacity: 0.7,
      fadeWhenIdle: false,
      keyLabel: key.label,
      hovering: c.input.hovering,
      target: phase === "idle" ? null : "Notepad",
      live: { stable: "", unstable: "" },
      shape: c.expected.shape,
      caption: c.expected.caption,
      window: c.expected.window,
    };
    return { state, seconds };
  };
  return [
    make("idle", 1.6, null),
    make("idle-hover", 1.3, null),
    make("ptt", 4.1, null),
    make("processing", 0.75, "transcribing"),
    make("processing", 0.4, "inserting"),
    make("done", 2.6, null, DEMO_TEXT),
  ];
}

const STYLES: WidgetStyle[] = ["flowPill", "captionBar", "island"];

interface Scenario {
  style: WidgetStyle;
  hotkeyId: string;
  model: ModelInfo;
  mic: MicResult;
  // Clicks to make after the page is up, as CSS selectors in order.
  clicks: string[];
}

function scenario(name: string): Scenario {
  const s: Scenario = {
    style: "flowPill",
    hotkeyId: "rightCtrl",
    model: { state: "ready", percent: null },
    mic: "ok",
    clicks: [],
  };
  const downloading: ModelInfo = { state: "downloading", percent: 42 };
  const tour = /^tour-(\d)(?:-(denied|downloading))?$/.exec(name);
  if (tour) {
    for (let i = 1; i < Number(tour[1]); i++) s.clicks.push("[data-continue]");
    if (tour[2] === "denied") {
      s.mic = "denied";
      s.clicks.push("[data-mic-test]");
    }
    if (tour[2] === "downloading") s.model = downloading;
    return s;
  }
  switch (name) {
    case "settings-general":
      break;
    case "settings-general-downloading":
      s.model = downloading;
      break;
    case "settings-general-picking":
      s.clicks.push("[data-pick-button]");
      break;
    case "settings-general-rightalt":
      s.hotkeyId = "rightAlt";
      break;
    case "settings-general-mic-silent":
      s.mic = "silent";
      s.clicks.push("[data-mic-test]");
      break;
    case "settings-about":
      s.clicks.push("[data-tab=about]");
      break;
    default: {
      const m = /^settings-appearance-(.+)$/.exec(name);
      const style = STYLES.find((x) => x === m?.[1]);
      if (!style) break;
      s.style = style;
      s.clicks.push("[data-tab=appearance]");
    }
  }
  return s;
}

// Clicks each selector once it exists. Short gaps, so a screenshot taken after a second sees the result.
function runClicks(selectors: string[], done: () => void) {
  const [first, ...rest] = selectors;
  if (!first) {
    done();
    return;
  }
  let tries = 0;
  const attempt = () => {
    const el = document.querySelector<HTMLElement>(first);
    if (el) {
      el.click();
      window.setTimeout(() => runClicks(rest, done), 120);
    } else if (++tries < 40) {
      window.setTimeout(attempt, 50);
    }
  };
  window.setTimeout(attempt, 150);
}

// Publishes <html data-layout-check="PASS ..."|"FAIL ..."> for settings-shots.sh (read with --dump-dom):
// nothing may overflow sideways, and a tour step must fit its window.
function scheduleLayoutCheck() {
  window.setTimeout(() => {
    const problems: string[] = [];
    const doc = document.documentElement;
    const root = document.getElementById("root");
    const width = root?.clientWidth ?? window.innerWidth;
    const height = root?.clientHeight ?? window.innerHeight;
    if (doc.scrollWidth > width + 1) problems.push(`sideways overflow ${doc.scrollWidth} > ${width}`);
    const step = document.querySelector<HTMLElement>(".sp-tour-step");
    if (step && step.scrollHeight > step.clientHeight + 1) problems.push(`tour step too tall by ${step.scrollHeight - step.clientHeight}px`);
    const tour = document.querySelector<HTMLElement>(".sp-tour");
    if (tour && tour.scrollHeight > tour.clientHeight + 1) problems.push(`tour taller than its window by ${tour.scrollHeight - tour.clientHeight}px`);
    for (const el of document.querySelectorAll<HTMLElement>(".sp-pane *, .sp-tour *")) {
      if (el.closest(".sp-demo")) continue;
      const r = el.getBoundingClientRect();
      if (r.width > 0 && (r.right > width + 1 || r.left < -1)) {
        problems.push(`${el.tagName.toLowerCase()}.${el.className} leaves the window sideways`);
        break;
      }
    }
    const pane = document.querySelector<HTMLElement>(".sp-pane");
    const scrolls = pane && pane.scrollHeight > pane.clientHeight + 1 ? `, pane scrolls ${pane.scrollHeight - pane.clientHeight}px` : "";
    doc.dataset.layoutCheck = problems.length ? `FAIL ${problems.join("; ")}` : `PASS ${width}x${height}${scrolls}`;
  }, 1200);
}

// The real window sizes (logical). Headless Chrome's viewport is shorter than the window it is given, so the
// page is forced to the size the native window would have.
function forceWindowSize(name: string) {
  const [width, height] = name.startsWith("tour") ? [620, 540] : [620, 600];
  for (const el of [document.documentElement, document.body, document.getElementById("root")]) {
    if (!el) continue;
    el.style.width = `${width}px`;
    el.style.height = `${height}px`;
  }
}

// Headless Chrome with a virtual clock does not advance CSS transitions and animations, so a shot would
// catch a step mid-fade and the dots mid-resize. ?still=1 jumps every one of them to its end state.
function stopAnimations() {
  const style = document.createElement("style");
  style.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
  document.head.appendChild(style);
}

export function createMockBackend(): Backend {
  const query = new URLSearchParams(window.location.search);
  const name = query.get("mock") ?? "";
  forceWindowSize(name);
  if (query.get("still") === "1") stopAnimations();
  const sc = scenario(name);
  const state = {
    widgetStyle: sc.style,
    glassOpacity: 0.7,
    fadeWhenIdle: true,
    widgetVisible: true,
    hotkeyId: sc.hotkeyId,
    model: sc.model,
  };
  const data = (): SettingsData => ({
    widgetStyle: state.widgetStyle,
    glassOpacity: state.glassOpacity,
    fadeWhenIdle: state.fadeWhenIdle,
    widgetVisible: state.widgetVisible,
    hotkey: hotkey(state.hotkeyId),
    model: state.model,
    version: "0.4.0",
  });

  const listeners = new Map<string, Set<(payload: never) => void>>();
  const emit = (event: string, payload: unknown) => listeners.get(event)?.forEach((fn) => fn(payload as never));

  // A real pick in the dev page: F13 to F24 are rare on a Mac, so Right Ctrl, Right Alt and Esc work too.
  let picking = false;
  window.addEventListener("keydown", (e) => {
    if (!picking) return;
    const id = e.code === "ControlRight" ? "rightCtrl" : e.code === "AltRight" ? "rightAlt" : /^F(1[3-9]|2[0-4])$/.test(e.code) ? e.code.toLowerCase() : null;
    const result: PickEvent | null = e.code === "Escape" ? { result: "cancelled", hotkey: null } : id ? { result: "picked", hotkey: hotkey(id) } : null;
    if (!result) return;
    picking = false;
    if (id) state.hotkeyId = id;
    emit("hotkey-pick", result);
    if (id) emit("settings-changed", data());
  });

  const handlers: Record<string, (args?: Record<string, unknown>) => unknown> = {
    get_settings: () => data(),
    update_settings: (args) => {
      const patch = (args?.patch ?? {}) as SettingsPatch;
      if (patch.widgetStyle) state.widgetStyle = patch.widgetStyle;
      if (patch.glassOpacity !== undefined) state.glassOpacity = patch.glassOpacity;
      if (patch.fadeWhenIdle !== undefined) state.fadeWhenIdle = patch.fadeWhenIdle;
      if (patch.widgetVisible !== undefined) state.widgetVisible = patch.widgetVisible;
      if (patch.hotkey) state.hotkeyId = patch.hotkey;
      return data();
    },
    start_hotkey_pick: () => {
      picking = true;
    },
    cancel_hotkey_pick: () => {
      picking = false;
    },
    check_microphone: () =>
      new Promise<MicCheck>((resolve) => window.setTimeout(() => resolve({ result: sc.mic, detail: null }), 300)),
    widget_demo: (args) => demoSteps(String(args?.style) as WidgetStyle, hotkey(state.hotkeyId)),
  };

  // The check runs once the scripted clicks are done and the last animation has settled.
  runClicks(sc.clicks, scheduleLayoutCheck);

  return {
    invoke: async <T>(command: string, args?: Record<string, unknown>) => {
      const handler = handlers[command];
      // Commands without a handler (open_microphone_privacy, finish_tour, ...) do nothing in the browser.
      const result = handler ? await handler(args) : undefined;
      return result as T;
    },
    listen: async <T>(event: string, handler: (payload: T) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(handler as (payload: never) => void);
      listeners.set(event, set);
      return () => {
        set.delete(handler as (payload: never) => void);
      };
    },
  };
}
