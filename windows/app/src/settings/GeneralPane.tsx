import { Group, Row, Segmented, Stepper, Toggle } from "../shared/controls";
import HotkeyPicker from "../shared/HotkeyPicker";
import MicrophoneCheck from "../shared/MicrophoneCheck";
import ModelStatus from "../shared/ModelStatus";
import type { PaneProps } from "../shared/useSettings";

const MODES = [
  { value: "raw" as const, label: "Raw" },
  { value: "clean" as const, label: "Clean" },
  { value: "prompt" as const, label: "Prompt" },
];

export default function GeneralPane({ settings, update }: PaneProps) {
  return (
    <>
      <Group title="Dictation">
        <HotkeyPicker hotkey={settings.hotkey} onUseDefault={() => update({ hotkey: "rightCtrl" })} />
        <Row label="Recognizer" detail="Parakeet v3, about 670 MB. Runs on this PC.">
          <ModelStatus model={settings.model} />
        </Row>
        <Row label="Default mode" detail="Raw inserts what was heard. Clean fixes the text. Prompt shapes it for an AI chat.">
          <Segmented
            label="Default mode"
            options={MODES}
            value={settings.defaultMode}
            onChange={(defaultMode) => update({ defaultMode })}
          />
        </Row>
        <Row label={`Prompt mode in AI apps from ${settings.promptModeMinWords} words`}>
          <Stepper
            label="Prompt mode words"
            value={settings.promptModeMinWords}
            min={10}
            max={200}
            step={5}
            onChange={(promptModeMinWords) => update({ promptModeMinWords })}
          />
        </Row>
      </Group>
      <Group title="Widget">
        <Row label="Show floating widget" detail="The pill at the bottom of the screen.">
          <Toggle label="Show floating widget" checked={settings.widgetVisible} onChange={(v) => update({ widgetVisible: v })} />
        </Row>
      </Group>
      <Group title="Microphone">
        <MicrophoneCheck />
      </Group>
    </>
  );
}
