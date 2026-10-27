import { useEffect, useRef, useState } from "react";
import { Group, Row, Segmented, Stepper } from "../shared/controls";
import ProviderList from "../shared/ProviderList";
import type { ClaudeModel } from "../shared/api";
import type { PaneProps } from "../shared/useSettings";

const MODELS = [
  { value: "haiku" as const, label: "Haiku" },
  { value: "sonnet" as const, label: "Sonnet" },
];

// A text field that saves when it loses focus or on Enter, so a half-typed path is never applied (a
// polishing change restarts the agent). The parent keys it by the saved value, so a stored change resets it.
function ExecutableField({ label, value, onSave }: { label: string; value: string; onSave: (v: string) => void }) {
  const [text, setText] = useState(value);
  const save = () => text !== value && onSave(text.trim());
  return (
    <input
      className="sp-input"
      aria-label={label}
      placeholder="Found automatically"
      spellCheck={false}
      value={text}
      onChange={(e) => setText(e.currentTarget.value)}
      onBlur={save}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
    />
  );
}

export default function PolishingPane({ settings, update }: PaneProps) {
  // The notes save a moment after typing stops, and at once on blur.
  const [notes, setNotes] = useState(settings.speakerNotes);
  const timer = useRef<number | undefined>(undefined);
  const saved = useRef(settings.speakerNotes);
  useEffect(() => {
    saved.current = settings.speakerNotes;
  }, [settings.speakerNotes]);
  const saveNotes = (v: string) => {
    window.clearTimeout(timer.current);
    if (v !== saved.current) void update({ speakerNotes: v });
  };
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const props = { settings, update };
  return (
    <>
      <Group title="Polishing">
        <div className="sp-block">
          <ProviderList {...props} />
        </div>
        <div className="sp-block-note">
          <p className="sp-note">
            Recognition stays on this PC. Polishing sends the recognized text, never the audio, to the agent you pick.
          </p>
        </div>
      </Group>
      {settings.polishing === "claude" && (
        <Group title="Claude Code">
          <Row label="Model">
            <Segmented
              label="Claude model"
              options={MODELS}
              value={settings.claudeModel}
              onChange={(v: ClaudeModel) => void update({ claudeModel: v })}
            />
          </Row>
          <Row label="Executable">
            <ExecutableField
              key={settings.claudeExecutable}
              label="Claude executable"
              value={settings.claudeExecutable}
              onSave={(v) => void update({ claudeExecutable: v })}
            />
          </Row>
        </Group>
      )}
      {settings.polishing === "codex" && (
        <Group title="Codex">
          <Row label="Executable">
            <ExecutableField
              key={settings.codexExecutable}
              label="Codex executable"
              value={settings.codexExecutable}
              onSave={(v) => void update({ codexExecutable: v })}
            />
          </Row>
        </Group>
      )}
      <Group>
        <Row
          label={`Skip polishing for phrases up to ${settings.fastPathMaxWords} words`}
          detail="Outside AI apps. Dictionary terms are still fixed."
        >
          <Stepper
            label="Skip polishing words"
            value={settings.fastPathMaxWords}
            min={0}
            max={30}
            onChange={(v) => void update({ fastPathMaxWords: v })}
          />
        </Row>
      </Group>
      <Group title="About the speaker">
        <div className="sp-block">
          <p className="sp-note">
            Goes into the agent&apos;s instructions: accent, sounds the recognizer gets wrong, habits. Any language.
          </p>
          <textarea
            className="sp-input"
            aria-label="About the speaker"
            rows={4}
            spellCheck={false}
            value={notes}
            onChange={(e) => {
              const v = e.currentTarget.value;
              setNotes(v);
              window.clearTimeout(timer.current);
              timer.current = window.setTimeout(() => saveNotes(v), 600);
            }}
            onBlur={() => saveNotes(notes)}
          />
        </div>
      </Group>
    </>
  );
}
