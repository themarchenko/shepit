import { Group, Row, Toggle } from "../shared/controls";
import HotkeyPicker from "../shared/HotkeyPicker";
import MicrophoneCheck from "../shared/MicrophoneCheck";
import ModelStatus from "../shared/ModelStatus";
import type { PaneProps } from "../shared/useSettings";

export default function GeneralPane({ settings, update }: PaneProps) {
  return (
    <>
      <Group title="Dictation">
        <HotkeyPicker hotkey={settings.hotkey} onUseDefault={() => update({ hotkey: "rightCtrl" })} />
        <Row label="Recognizer" detail="Parakeet v3, about 670 MB. Runs on this PC.">
          <ModelStatus model={settings.model} />
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
