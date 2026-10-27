import type { WidgetStyle } from "../widgetState";
import { Segmented, type Option } from "./controls";
import DemoStage from "./DemoStage";
import { useDemo } from "./widgetDemo";

// Names and blurbs from WidgetStyle in apple/ShepitCore/.../AppSettings.swift.
export const STYLES: (Option<WidgetStyle> & { blurb: string })[] = [
  { value: "flowPill", label: "Flow pill", blurb: "A dash that springs into a pill. Words float above it." },
  { value: "captionBar", label: "Caption bar", blurb: "A wide bar that reads like subtitles. Best for long prompts." },
  { value: "island", label: "Island", blurb: "One shape that grows into a card with the text and steps." },
];

// The three styles over one live preview of the selected one.
export default function StylePicker({
  style,
  glassOpacity,
  hotkeyId,
  stageHeight = 150,
  onChange,
}: {
  style: WidgetStyle;
  glassOpacity: number;
  hotkeyId: string;
  stageHeight?: number;
  onChange: (style: WidgetStyle) => void;
}) {
  const demo = useDemo(style, hotkeyId, glassOpacity);
  const blurb = STYLES.find((s) => s.value === style)?.blurb ?? "";
  return (
    <div className="sp-picker">
      <Segmented options={STYLES} value={style} onChange={onChange} label="Widget style" />
      <DemoStage steps={demo.steps} state={demo.state} elapsed={demo.elapsed} height={stageHeight} />
      <p className="sp-note">{blurb}</p>
    </div>
  );
}
