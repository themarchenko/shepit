// Dev only (see installDevBackendIfNeeded): answers the commands in a plain browser so every Settings and
// tour state can be rendered and checked, and scripts a few clicks for states that need them. Not part of
// the production bundle. States: ?mock=settings-{general,general-downloading,general-picking,
// general-rightalt,general-mic-silent,appearance-<style>,polishing,polishing-none,polishing-codex,about,
// about-available,about-checking,about-failed,dictionary,dictionary-empty}, ?mock=history{,-empty,-many,
// -error,-clear-confirm}, ?mock=tour-{1,2,2-denied,3,4,5,5-downloading} or
// ?mock=update-<case> for every case of the update card fixture (update-cards.json, the file Rust is tested against).
import fixture from "../../../core/tests/fixtures/widget-layout-windows.json";
import type { WidgetPhase, WidgetStage, WidgetState, WidgetStyle } from "../widgetState";
import cards from "../../src-tauri/tests/fixtures/update-cards.json";
import type { AgentsInfo, Backend, CardPayload, DemoStep, HistoryItem, HistoryPayload, HotkeyInfo, MicCheck, MicResult, ModelInfo, PickEvent, Polishing, SettingsData, SettingsPatch, UpdateStatus } from "./api";

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

const TERMS = ["Supabase", "Vercel", "Tauri", "WebView2", "Parakeet", "kubectl", "useEffect", "Claude Code"];

// The splitting rule of dictionary::normalize_terms.
const normalizeTerms = (terms: string[]) =>
  terms.flatMap((t) => t.split(/\r\n|\r|\n/)).map((t) => t.trim()).filter((t) => t !== "");

const SOURCES = ["claude", "codex", "passthrough", "fastPath", "fallback"];
const LONG_RAW =
  "so basically what I wanted to say is that the deploy pipeline keeps failing on the staging branch because the migration step runs before the database is ready and then the whole job times out after ten minutes and nobody notices until the next morning when the build badge is red and somebody has to dig through the logs to find out what happened";

// Every row is a state the app can produce: raw never reaches the agent (passthrough, no agent time),
// a fast-path row has no agent time, a fallback row carries the time spent before the agent gave up,
// and the profile ids are the real ones (windows/core/src/profile.rs).
const PROFILES = ["generic", "ai-chat", "messenger"];

function historyItem(id: number, createdAtMs: number, n: number): HistoryItem {
  const source = SOURCES[n % SOURCES.length];
  const apps: [string | null, string | null][] = [["Visual Studio Code", "Code.exe"], [null, "slack.exe"], [null, null], ["Notepad", "notepad.exe"]];
  const [appName, appId] = apps[n % apps.length];
  const raw = n === 2 ? LONG_RAW : n % 2 ? "ok so let's meet tomorrow at ten" : "Зустрінемось завтра о десятій";
  const profile = PROFILES[n % PROFILES.length];
  const mode = source === "passthrough" && n % 2 === 0 ? "raw" : source === "claude" && profile === "ai-chat" ? "prompt" : "clean";
  const formatted = n === 2 ? `${LONG_RAW.charAt(0).toUpperCase()}${LONG_RAW.slice(1)}.` : n % 2 ? "OK, so let's meet tomorrow at 10." : "Зустрінемось завтра о десятій.";
  return {
    id,
    createdAtMs,
    appName,
    appId,
    profile,
    mode,
    formatterSource: source,
    sttMs: 180 + n * 7,
    llmMs: source === "passthrough" || source === "fastPath" ? 0 : 600 + n * 11,
    rawText: raw,
    formattedText: mode === "raw" ? raw : formatted,
    status: "inserted",
  };
}

function historyPayload(name: string): HistoryPayload {
  const now = Date.now();
  const yesterday = new Date();
  yesterday.setDate(yesterday.getDate() - 1);
  yesterday.setHours(18, 42, 0, 0);
  if (name === "history-empty") return { items: [], total: 0, error: null };
  if (name === "history-error") return { items: [], total: 0, error: "History is unavailable" };
  if (name === "history-many") {
    return { items: Array.from({ length: 200 }, (_, i) => historyItem(1000 - i, now - i * 90_000, i)), total: 1234, error: null };
  }
  const items = Array.from({ length: 7 }, (_, i) => historyItem(10 - i, i < 3 ? now - (i * 9 + 2) * 60_000 : yesterday.getTime() - (i - 3) * 3_600_000, i));
  return { items, total: items.length, error: null };
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
    case "settings-dictionary":
    case "settings-dictionary-empty":
      s.clicks.push("[data-tab=dictionary]");
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
    case "history-clear-confirm":
      s.clicks.push("[data-clear]");
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
function historyProblems(expected: HistoryPayload, name: string): string[] {
  const problems: string[] = [];
  const rows = document.querySelectorAll<HTMLElement>(".hi-row");
  if (rows.length !== expected.items.length) problems.push(`${rows.length} rows != ${expected.items.length}`);
  const count = document.querySelector("[data-count]")?.textContent ?? "";
  const wantNote = expected.total > expected.items.length;
  if (wantNote !== count.startsWith("Showing the newest")) problems.push(`count line "${count}" is wrong for total ${expected.total}`);
  if (wantNote && !count.endsWith(` of ${expected.total}`)) problems.push(`count line "${count}" lacks the total`);
  if ((document.querySelector(".hi-empty") !== null) !== (expected.items.length === 0 && !expected.error)) problems.push("empty state is wrong");
  if ((document.querySelector(".hi-error") !== null) !== (expected.error !== null)) problems.push("error line is wrong");
  const confirm = document.querySelector(".hi-confirm")?.textContent;
  if ((name === "history-clear-confirm") !== (confirm !== undefined)) problems.push("confirmation is wrong");
  if (confirm !== undefined && !confirm.startsWith(`Delete all ${expected.total} dictations`)) problems.push(`confirmation says "${confirm}"`);
  for (const raw of document.querySelectorAll<HTMLElement>(".hi-raw")) {
    if (raw.clientHeight > 3 * 20) problems.push("a raw text shows more than 3 lines");
  }
  const list = document.querySelector<HTMLElement>(".hi-list");
  if (list && rows.length > 0 && list.clientHeight < 40) problems.push("the list has no room");
  return problems;
}

function scheduleLayoutCheck(resizes: number[], expectCard: CardPayload | null, expectHistory: HistoryPayload | null, expectTerms: string[] | null, name: string) {
  window.setTimeout(() => {
    const problems: string[] = [];
    if (expectCard) problems.push(...cardProblems(resizes, expectCard));
    if (expectHistory) problems.push(...historyProblems(expectHistory, name));
    if (expectTerms) {
      const box = document.querySelector<HTMLTextAreaElement>(".sp-terms");
      if (!box) problems.push("no dictionary text area");
      else if (box.value !== expectTerms.join("\n")) problems.push(`dictionary text "${box.value}" != the stored terms`);
    }
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
    for (const el of document.querySelectorAll<HTMLElement>(".sp-pane *, .sp-tour *, .hi-window *")) {
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
  const [width, height] = name.startsWith("update") ? [420, 320] : name.startsWith("tour") ? [620, 540] : name.startsWith("history") ? [760, 520] : [620, 600];
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
    dictionary: name === "settings-dictionary-empty" ? [] : name === "settings-dictionary" ? [...TERMS] : [],
    history: historyPayload(name),
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
    version: "0.3.1",
    polishing: state.polishing,
    claudeModel: state.claudeModel,
    claudeExecutable: state.claudeExecutable,
    codexExecutable: state.codexExecutable,
    defaultMode: state.defaultMode,
    fastPathMaxWords: state.fastPathMaxWords,
    promptModeMinWords: state.promptModeMinWords,
    speakerNotes: state.speakerNotes,
    checkForUpdates: state.checkForUpdates,
    dictionary: state.dictionary,
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
      if (patch.dictionary !== undefined) state.dictionary = normalizeTerms(patch.dictionary);
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
    history_recent: () => state.history,
    history_clear: () => {
      state.history = { items: [], total: 0, error: state.history.error };
      emit("history-changed", null);
    },
    update_card_ready: () => card,
    update_resize: (args) => {
      resizes.push(Number(args?.height));
    },
    widget_demo: (args) => demoSteps(String(args?.style) as WidgetStyle, hotkey(state.hotkeyId)),
  };

  // The check runs once the scripted clicks are done and the last animation has settled.
  const expectHistory = name.startsWith("history") ? state.history : null;
  const expectTerms = name.startsWith("settings-dictionary") ? state.dictionary : null;
  runClicks(sc.clicks, () => scheduleLayoutCheck(resizes, card, expectHistory, expectTerms, name));

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
