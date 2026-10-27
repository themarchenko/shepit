import { useState } from "react";
import { Button, Group, Row, Slider, Toggle } from "../shared/controls";
import { resetWidgetPosition } from "../shared/api";
import StylePicker from "../shared/StylePicker";
import type { PaneProps } from "../shared/useSettings";

export default function AppearancePane({ settings, update }: PaneProps) {
  // While the slider moves the preview follows it; the value is saved when it is let go.
  const [dragging, setDragging] = useState<number | null>(null);
  const glass = dragging ?? settings.glassOpacity;

  return (
    <>
      <Group title="Widget style">
        <div style={{ padding: 12 }}>
          <StylePicker
            style={settings.widgetStyle}
            glassOpacity={glass}
            hotkeyId={settings.hotkey.id}
            onChange={(widgetStyle) => update({ widgetStyle })}
          />
        </div>
      </Group>
      <Group>
        <Row label="Glass">
          <Slider
            label="Glass"
            value={glass}
            min={0.5}
            max={0.9}
            step={0.01}
            minLabel="Lighter"
            maxLabel="Darker"
            onChange={setDragging}
            onCommit={(v) => {
              setDragging(null);
              if (v !== settings.glassOpacity) void update({ glassOpacity: v });
            }}
          />
        </Row>
        <Row label="Fade when idle" detail="Down to about half after 3 seconds.">
          <Toggle label="Fade when idle" checked={settings.fadeWhenIdle} onChange={(v) => update({ fadeWhenIdle: v })} />
        </Row>
        <Row label="Position" detail="Drag the widget anywhere; it stays there across launches.">
          <Button onClick={() => resetWidgetPosition().catch(() => {})}>Reset position</Button>
        </Row>
      </Group>
    </>
  );
}
