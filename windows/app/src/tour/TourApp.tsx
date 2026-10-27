import { useEffect, useRef, useState, type ReactNode } from "react";
import { finishTour } from "../shared/api";
import { Group, KeyCap, PrimaryButton, QuietButton } from "../shared/controls";
import DemoStage from "../shared/DemoStage";
import HotkeyPicker from "../shared/HotkeyPicker";
import MicrophoneCheck from "../shared/MicrophoneCheck";
import ModelStatus from "../shared/ModelStatus";
import ProviderList from "../shared/ProviderList";
import StylePicker from "../shared/StylePicker";
import { useSettings, type PaneProps } from "../shared/useSettings";
import { useDemo } from "../shared/widgetDemo";

const STEP_COUNT = 5;
const ESC_AFTER_PICK_MS = 400;

// The window has no title bar. A press on a plain area drags it; this attribute marks those areas.
const DRAG = { "data-tauri-drag-region": true } as const;

function Welcome({ settings, update, onPickingChange }: PaneProps & { onPickingChange: (p: boolean) => void }) {
  const demo = useDemo(settings.widgetStyle, settings.hotkey.id, settings.glassOpacity);
  const [changing, setChanging] = useState(false);
  // The demo's last step carries the text it "typed"; the box shows it while that step plays.
  const typed = demo.state?.phase === "done" ? demo.state.message : "";

  return (
    <>
      <h1 {...DRAG}>Speak instead of typing</h1>
      <p className="sp-tour-text" {...DRAG}>
        Hold <KeyCap label={settings.hotkey.label} />, say what you want to write, let go.
      </p>
      <p className="sp-tour-text" {...DRAG}>
        Shepit types it into the app you are in, in Ukrainian or English.
      </p>
      {changing ? (
        <Group>
          <HotkeyPicker
            hotkey={settings.hotkey}
            onUseDefault={() => update({ hotkey: "rightCtrl" })}
            onPickingChange={onPickingChange}
          />
        </Group>
      ) : (
        <>
          <div className="sp-field" aria-hidden="true">
            {typed}
            <span className="sp-field-caret" />
          </div>
          <DemoStage steps={demo.steps} state={demo.state} elapsed={demo.elapsed} height={150} maxScale={0.8} />
        </>
      )}
      <div className="sp-tour-link">
        <QuietButton data-change-key onClick={() => setChanging((c) => !c)}>
          {changing ? "Back to the demo" : `No ${settings.hotkey.label}? Change key`}
        </QuietButton>
      </div>
    </>
  );
}

function Microphone() {
  return (
    <>
      <h1 {...DRAG}>Check your microphone</h1>
      <p className="sp-tour-text" {...DRAG}>
        Speech recognition runs on this PC. The audio never leaves it.
      </p>
      <Group>
        <MicrophoneCheck />
      </Group>
    </>
  );
}

function Look({ settings, update }: PaneProps) {
  return (
    <>
      <h1 {...DRAG}>Pick a look</h1>
      <p className="sp-tour-text" {...DRAG}>
        The widget sits at the bottom of the screen. Drag it anywhere and it stays there. You can switch later in
        Settings.
      </p>
      <StylePicker
        style={settings.widgetStyle}
        glassOpacity={settings.glassOpacity}
        hotkeyId={settings.hotkey.id}
        onChange={(widgetStyle) => update({ widgetStyle })}
      />
    </>
  );
}

function Polish({ settings, update }: PaneProps) {
  return (
    <>
      <h1 {...DRAG}>Polish the text</h1>
      <p className="sp-tour-text" {...DRAG}>
        An AI agent you already use can fix punctuation, names and code terms. It gets the text only, never the
        audio.
      </p>
      <ProviderList settings={settings} update={update} />
    </>
  );
}

function TryIt({ settings }: PaneProps) {
  const box = useRef<HTMLTextAreaElement>(null);
  // A dictation would only wait for the download, so the box stays off until the model is there.
  const downloading = settings.model.state === "downloading";
  useEffect(() => {
    if (!downloading) box.current?.focus();
  }, [downloading]);
  return (
    <div className={downloading ? "sp-tour-waiting" : undefined} style={{ display: "contents" }}>
      <h1 {...DRAG}>Try it</h1>
      <p className="sp-tour-text" {...DRAG}>
        Click the box, hold <KeyCap label={settings.hotkey.label} /> and say a sentence.
      </p>
      <textarea
        ref={box}
        className="sp-field"
        aria-label="Try dictating here"
        spellCheck={false}
        disabled={downloading}
      />
      <div className="sp-tour-model">
        <ModelStatus model={settings.model} />
        {downloading && (
          <p className="sp-note">The first dictation waits for the speech model to finish downloading.</p>
        )}
      </div>
    </div>
  );
}

export default function TourApp() {
  const { settings, update } = useSettings();
  const [step, setStep] = useState(0);
  const [picking, setPicking] = useState(false);
  // When the last key pick ended. The hook ends a pick on Esc and the page can learn of it before the
  // same Esc reaches the DOM, so that Esc must not also close the tour.
  const pickEndedAt = useRef(0);
  const onPickingChange = (p: boolean) => {
    if (!p) pickEndedAt.current = performance.now();
    setPicking(p);
  };

  const last = step === STEP_COUNT - 1;
  const next = () => (last ? void finishTour() : setStep((s) => s + 1));

  // Esc closes the tour (and marks it done) unless a key is being picked, where Esc cancels the pick.
  // Enter goes on, except in the text box, where it belongs to the text, and on a button, which
  // already acts on Enter.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (picking) return;
      if (e.key === "Escape") {
        // Not a held key, and not the Esc that just ended a pick.
        if (e.repeat || performance.now() - pickEndedAt.current < ESC_AFTER_PICK_MS) return;
        void finishTour();
      } else if (e.key === "Enter") {
        const tag = (e.target as HTMLElement | null)?.tagName;
        if (tag === "TEXTAREA" || tag === "INPUT" || tag === "BUTTON") return;
        next();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!settings) return null;
  const props = { settings, update };
  let body: ReactNode;
  if (step === 0) body = <Welcome {...props} onPickingChange={onPickingChange} />;
  else if (step === 1) body = <Microphone />;
  else if (step === 2) body = <Look {...props} />;
  else if (step === 3) body = <Polish {...props} />;
  else body = <TryIt {...props} />;

  return (
    <div className="sp-tour" {...DRAG}>
      <div className="sp-tour-logo" {...DRAG}>
        <svg width="20" height="14" viewBox="0 0 20 14" fill="currentColor" aria-hidden="true">
          {[5, 9, 13, 8, 4].map((h, i) => (
            <rect key={i} x={i * 4.2} y={7 - h / 2} width="2.4" height={h} rx="1.2" />
          ))}
        </svg>
        Shepit
      </div>
      <QuietButton className="sp-tour-skip" onClick={() => void finishTour()}>
        Skip
      </QuietButton>
      <div key={step} className="sp-tour-step" {...DRAG}>
        {body}
      </div>
      <div className="sp-tour-foot" {...DRAG}>
        <div className="sp-tour-dots" aria-hidden="true">
          {Array.from({ length: STEP_COUNT }, (_, i) => (
            <i key={i} className={i === step ? "on" : ""} />
          ))}
        </div>
        <div className="sp-tour-btns">
          {step > 0 && <QuietButton onClick={() => setStep(step - 1)}>Back</QuietButton>}
          <PrimaryButton data-continue onClick={next}>
            {last ? "Start dictating" : "Continue"}
          </PrimaryButton>
        </div>
      </div>
    </div>
  );
}
