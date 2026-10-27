// Dev only (see installDevBackendIfNeeded): answers the commands in a plain browser so every Settings and
// tour state can be rendered and checked, and scripts a few clicks for states that need them. Not part of
// the production bundle. States: ?mock=settings-{general,general-downloading,general-picking,
// general-rightalt,general-mic-silent,appearance-<style>,polishing,polishing-none,polishing-codex,about,
// about-available,about-checking,about-failed}, ?mock=tour-{1,2,2-denied,3,4,5,5-downloading} or
// ?mock=update-<case> for every case of the update card fixture (update-cards.json, the file Rust is tested against).
import fixture from "../../../core/tests/fixtures/widget-layout-windows.json";
import type { WidgetPhase, WidgetStage, WidgetState, WidgetStyle } from "../widgetState";
import cards from "../../src-tauri/tests/fixtures/update-cards.json";
import type { AgentsInfo, Backend, CardPayload, DemoStep, HotkeyInfo, MicCheck, MicResult, ModelInfo, PickEvent, Polishing, SettingsData, SettingsPatch, UpdateStatus } from "./api";

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
    make("processing", 1.15, "formatting"),
    make("processing", 0.4, "inserting"),
    make("done", 2.6, null, DEMO_TEXT),
  ];
}

const CARDS = cards.cases as Record<string, { card: CardPayload }>;

// The About pane's update row for the three scripted states; other mocks show an idle app, checked 3 h ago.
function aboutStatus(name: string): UpdateStatus {
  const ago = Math.floor(Date.now() / 1000) - 3 * 3600;
  switch (name) {
    case "settings-about-available":
      return { state: "available", label: "Shepit 0.4.0 is out", message: null, lastCheck: ago };
    case "settings-about-checking":
      return { state: "checking", label: "Checking GitHub\u2026", message: null, lastCheck: ago };
    case "settings-about-failed":
      return { state: "failed", label: "Last check failed", message: "Could not reach GitHub.", lastCheck: ago };
    default:
      return { state: "upToDate", label: "Up to date", message: null, lastCheck: ago };
  }
}

const STYLES: WidgetStyle[] = ["flowPill", "captionBar", "island"];

interface Scenario {
  style: WidgetStyle;
  hotkeyId: string;
  model: ModelInfo;
  mic: MicResult;
  // The agents the mock "finds" on the PC, and the stored choice before the fallback rule runs.
  found: { claude: boolean; codex: boolean };
  polishing: Polishing;
  // Clicks to make after the page is up, as CSS selectors in order.
  clicks: string[];
}

function scenario(name: string): Scenario {
  const s: Scenario = {
    style: "flowPill",
    hotkeyId: "rightCtrl",
    model: { state: "ready", percent: null },
    mic: "ok",
    found: { claude: true, codex: true },
    polishing: "claude",
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
    case "settings-polishing":
      s.clicks.push("[data-tab=polishing]");
      break;
    case "settings-polishing-none":
      // Claude is chosen but nothing is installed: the fallback rule must turn polishing off.
      s.found = { claude: false, codex: false };
      s.clicks.push("[data-tab=polishing]");
      break;
    case "settings-polishing-codex":
      s.found = { claude: false, codex: true };
      s.clicks.push("[data-tab=polishing]");
      break;
    case "settings-about":
    case "settings-about-available":
    case "settings-about-checking":
    case "settings-about-failed":
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

// The update card: it must ask for its own height plus the margins, fit the 420x320 window, and draw the
// indeterminate bar exactly when Rust sends a null fraction.
function cardProblems(resizes: number[], expected: CardPayload): string[] {
  const problems: string[] = [];
  const card = document.querySelector<HTMLElement>(".up-card");
  if (!card) return ["no card drawn"];
  const h = Math.ceil(card.offsetHeight);
  if (resizes.length === 0) problems.push("update_resize was never called");
  else if (resizes[resizes.length - 1] !== h + 40) problems.push(`update_resize ${resizes[resizes.length - 1]} != card ${h} + 40`);
  if (h + 40 > 320) problems.push(`card ${h} + 40 does not fit 320`);
  if (card.offsetWidth !== 380) problems.push(`card is ${card.offsetWidth} wide, not 380`);
  const bar = document.querySelector(".up-bar");
  const wantBar = expected.progress !== null;
  if (!!bar !== wantBar) problems.push(wantBar ? "progress bar missing" : "progress bar not expected");
  const indeterminate = !!bar?.classList.contains("indeterminate");
  if (wantBar && indeterminate !== (expected.progress?.fraction === null)) problems.push(`indeterminate bar is ${indeterminate}`);
  const title = card.querySelector(".up-title")?.textContent;
  if (title !== expected.title) problems.push(`title "${title}" != "${expected.title}"`);
  const shown = [...card.querySelectorAll("button")].map((b) => b.textContent);
  const want = [...(expected.link ? [expected.link.label] : []), ...[...expected.buttons].sort((a, b) => Number(b.lead) - Number(a.lead)).map((b) => b.label)];
  if (shown.join("|") !== want.join("|")) problems.push(`buttons ${shown.join(",")} != ${want.join(",")}`);
  for (const el of card.querySelectorAll<HTMLElement>("*")) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.right > card.getBoundingClientRect().right - 15) {
      problems.push(`${el.tagName.toLowerCase()}.${el.className} crowds the card edge`);
      break;
    }
  }
  return problems;
}

// Publishes <html data-layout-check="PASS ..."|"FAIL ..."> for settings-shots.sh (read with --dump-dom):
// nothing may overflow sideways, and a tour step must fit its window.
function scheduleLayoutCheck(resizes: number[], expectCard: CardPayload | null) {
  window.setTimeout(() => {
    const problems: string[] = [];
    if (expectCard) problems.push(...cardProblems(resizes, expectCard));
    const doc = document.documentElement;
    const root = document.getElementById("root");
    const width = root?.clientWidth ?? window.innerWidth;
    const height = root?.clientHeight ?? window.innerHeight;
    // Headless Chrome will not make a window narrower than 500 px, so the card page is judged by its own edges.
    if (!expectCard && doc.scrollWidth > width + 1) problems.push(`sideways overflow ${doc.scrollWidth} > ${width}`);
    const step = document.querySelector<HTMLElement>(".sp-tour-step");
    if (step && step.scrollHeight > step.clientHeight + 1) problems.push(`tour step too tall by ${step.scrollHeight - step.clientHeight}px`);
    const tour = document.querySelector<HTMLElement>(".sp-tour");
    if (tour && tour.scrollHeight > tour.clientHeight + 1) problems.push(`tour taller than its window by ${tour.scrollHeight - tour.clientHeight}px`);
    // The fallback rule: the chosen agent is never one that was not found.
    const chosen = document.querySelector<HTMLButtonElement>(".sp-provider[aria-checked=true]");
    if (chosen?.disabled) problems.push(`chosen agent ${chosen.dataset.provider} is not installed`);
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
  const [width, height] = name.startsWith("update") ? [420, 320] : name.startsWith("tour") ? [620, 540] : [620, 600];
  // The card window is transparent; a mid-grey desktop behind it shows the glass.
  if (name.startsWith("update")) document.documentElement.style.background = "#4a5568";
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
    polishing: sc.polishing,
    claudeModel: "haiku" as SettingsData["claudeModel"],
    claudeExecutable: "",
    codexExecutable: "",
    defaultMode: "clean" as SettingsData["defaultMode"],
    fastPathMaxWords: 8,
    promptModeMinWords: 40,
    speakerNotes: "",
    checkForUpdates: true,
    update: aboutStatus(name),
  };
  const resizes: number[] = [];
  const cardName = /^update-(.+)$/.exec(name)?.[1];
  const card = cardName ? CARDS[cardName]?.card ?? null : null;
  const data = (): SettingsData => ({
    widgetStyle: state.widgetStyle,
    glassOpacity: state.glassOpacity,
    fadeWhenIdle: state.fadeWhenIdle,
    widgetVisible: state.widgetVisible,
    hotkey: hotkey(state.hotkeyId),
    model: state.model,
    version: "0.4.0",
    polishing: state.polishing,
    claudeModel: state.claudeModel,
    claudeExecutable: state.claudeExecutable,
    codexExecutable: state.codexExecutable,
    defaultMode: state.defaultMode,
    fastPathMaxWords: state.fastPathMaxWords,
    promptModeMinWords: state.promptModeMinWords,
    speakerNotes: state.speakerNotes,
    checkForUpdates: state.checkForUpdates,
  });
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Math.round(v)));

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
      if (patch.polishing) state.polishing = patch.polishing;
      if (patch.claudeModel) state.claudeModel = patch.claudeModel;
      if (patch.claudeExecutable !== undefined) state.claudeExecutable = patch.claudeExecutable;
      if (patch.codexExecutable !== undefined) state.codexExecutable = patch.codexExecutable;
      if (patch.defaultMode) state.defaultMode = patch.defaultMode;
      if (patch.fastPathMaxWords !== undefined) state.fastPathMaxWords = clamp(patch.fastPathMaxWords, 0, 30);
      if (patch.promptModeMinWords !== undefined) state.promptModeMinWords = clamp(patch.promptModeMinWords, 10, 200);
      if (patch.speakerNotes !== undefined) state.speakerNotes = patch.speakerNotes;
      if (patch.checkForUpdates !== undefined) state.checkForUpdates = patch.checkForUpdates;
      return data();
    },
    // The real command runs the same fallback rule server-side and re-sends the settings.
    detect_agents: (): AgentsInfo => {
      const claude = sc.found.claude ? "C:\\Users\\Leo\\.local\\bin\\claude.exe" : null;
      const codex = sc.found.codex ? "C:\\Users\\Leo\\AppData\\Roaming\\npm\\codex.cmd" : null;
      if (state.polishing !== "off" && !(state.polishing === "claude" ? claude : codex)) {
        state.polishing = claude ? "claude" : codex ? "codex" : "off";
        emit("settings-changed", data());
      }
      return { claude, codex, settings: data() };
    },
    start_hotkey_pick: () => {
      picking = true;
    },
    cancel_hotkey_pick: () => {
      picking = false;
    },
    check_microphone: () =>
      new Promise<MicCheck>((resolve) => window.setTimeout(() => resolve({ result: sc.mic, detail: null }), 300)),
    get_update_status: () => state.update,
    // A click shows the working state, then the result a moment later, like the real check.
    check_updates_now: () => {
      state.update = { state: "checking", label: "Checking GitHub\u2026", message: null, lastCheck: state.update.lastCheck };
      emit("update-status", state.update);
      window.setTimeout(() => {
        state.update = { state: "upToDate", label: "Up to date", message: null, lastCheck: Math.floor(Date.now() / 1000) };
        emit("update-status", state.update);
      }, 1200);
    },
    update_card_ready: () => card,
    update_resize: (args) => {
      resizes.push(Number(args?.height));
    },
    widget_demo: (args) => demoSteps(String(args?.style) as WidgetStyle, hotkey(state.hotkeyId)),
  };

  // The check runs once the scripted clicks are done and the last animation has settled.
  runClicks(sc.clicks, () => scheduleLayoutCheck(resizes, card));

  return {
    invoke: async <T>(command: string, args?: Record<string, unknown>) => {
      const handler = handlers[command];
      // Commands without a handler (open_microphone_privacy, finish_tour, ...) do nothing in the browser.
      const result = handler ? await handler(args) : undefined;
      return result as T;
    },
    listen: async <T>(event: string, handler: (payload: T) => void) => {
      const set = listeners.get(event) ?? new Set();
      // A wrapper per subscription, like Tauri's: StrictMode subscribes the same stable function twice and
      // the first unsubscribe must not remove the second.
      const own = (payload: never) => handler(payload as T);
      set.add(own);
      listeners.set(event, set);
      return () => {
        set.delete(own);
      };
    },
  };
}
