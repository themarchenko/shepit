import CaptionBar from "./CaptionBar";
import FlowPill from "./FlowPill";
import Island from "./Island";
import type { WidgetState } from "./widgetState";

// The shape of the selected style for one state. Pure: no events, no commands. The widget window and the
// Settings and tour demos all draw through it.
export default function WidgetView({ state, elapsed }: { state: WidgetState; elapsed: string }) {
  const props = { s: state, elapsed };
  if (state.style === "captionBar") return <CaptionBar {...props} />;
  if (state.style === "island") return <Island {...props} />;
  return <FlowPill {...props} />;
}
