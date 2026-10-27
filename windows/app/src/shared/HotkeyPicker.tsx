import { useEffect, useRef, useState } from "react";
import { cancelHotkeyPick, onHotkeyPick, startHotkeyPick, type HotkeyInfo } from "./api";
import { Button, KeyCap, QuietButton } from "./controls";
import { useEvent } from "./useSettings";

// Records the next key press through the keyboard hook in Rust (the page only sees the result). A pick
// ends on a key, Esc, the 15 s limit, or when this window loses focus or the picker goes away.
export default function HotkeyPicker({
  hotkey,
  onUseDefault,
  onPickingChange,
}: {
  hotkey: HotkeyInfo;
  onUseDefault: () => void;
  onPickingChange?: (picking: boolean) => void;
}) {
  const [picking, setPicking] = useState(false);
  const [timedOut, setTimedOut] = useState(false);
  const pickingRef = useRef(false);

  const set = (value: boolean) => {
    pickingRef.current = value;
    setPicking(value);
    onPickingChange?.(value);
  };

  const start = () => {
    setTimedOut(false);
    set(true);
    startHotkeyPick().catch(() => set(false));
  };
  const cancel = () => {
    if (!pickingRef.current) return;
    set(false);
    cancelHotkeyPick().catch(() => {});
  };

  useEvent(() =>
    onHotkeyPick((e) => {
      if (!pickingRef.current) return;
      set(false);
      setTimedOut(e.result === "timedOut");
    }),
  );

  // Leaving the window, or this pane, ends the pick. Rust keeps no focus state of its own.
  const cancelRef = useRef(cancel);
  useEffect(() => {
    cancelRef.current = cancel;
  });
  useEffect(() => {
    const onBlur = () => cancelRef.current();
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("blur", onBlur);
      cancelRef.current();
    };
  }, []);

  // The hook does not swallow keys, so the page still sees them; keep them from doing anything here.
  useEffect(() => {
    if (!picking) return;
    const stop = (e: KeyboardEvent) => e.preventDefault();
    window.addEventListener("keydown", stop);
    return () => window.removeEventListener("keydown", stop);
  }, [picking]);

  return (
    <div className="sp-block">
      <div className="sp-block-head">
        <div className="sp-row-text">
          <span>Dictation key</span>
          <span className="sp-row-detail">Hold it to talk, double-tap for hands-free.</span>
        </div>
        <div className="sp-row-control">
          {picking ? (
            <>
              <KeyCap label="Press a key" picking />
              <Button onClick={cancel}>Cancel</Button>
            </>
          ) : (
            <>
              <KeyCap label={hotkey.label} />
              <Button data-pick-button onClick={start}>
                Change
              </Button>
            </>
          )}
        </div>
      </div>
      <p className="sp-note">Right Ctrl, Right Alt or F13 to F24. Esc cancels.</p>
      {timedOut && !picking && <p className="sp-note">No key was pressed, so the key stays {hotkey.label}.</p>}
      {hotkey.id === "rightAlt" && (
        <p className="sp-note">
          On keyboard layouts where Right Alt is AltGr (such as Ukrainian Enhanced), it can also type special
          characters. Right Ctrl is the safer choice.
        </p>
      )}
      {!hotkey.isDefault && !picking && (
        <div>
          <QuietButton onClick={onUseDefault}>Use Right Ctrl</QuietButton>
        </div>
      )}
    </div>
  );
}
