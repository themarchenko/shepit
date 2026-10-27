import { captionContent } from "./captionContent";
import { CheckIcon, ExclamationIcon, StopSquare } from "./Icons";
import FitRow from "./FitRow";
import KeyCap from "./KeyCap";
import LevelBars from "./LevelBars";
import LiveLine from "./LiveLine";
import { glassStyle, shapeStyle, type StyleProps } from "./styleProps";
import { MARGIN } from "./widgetState";

// 1. A dash at the bottom that springs into a pill with a mirrored waveform; words float in a caption above.
export default function FlowPill({ s }: StyleProps) {
  const content = s.caption ? captionContent(s) : null;
  const listening = s.phase === "listening";
  const processing = s.phase === "processing";
  const idleHover = s.phase === "idle" && s.hovering;

  return (
    <div className="flow">
      {content && (
        // The window is the widest the caption may be (its max width plus the shadow margin).
        <div className="glass caption-box" style={{ ...glassStyle(s), maxWidth: s.window.width - 2 * MARGIN.x }}>
          {content.label && <span className="caption-label">{content.label}</span>}
          <LiveLine {...content} />
        </div>
      )}
      <div className="glass shape pill" style={{ ...shapeStyle(s), ...glassStyle(s) }}>
        {idleHover && (
          <FitRow size={12} className="pill-hint fade-in">
            <span>Hold</span>
            <KeyCap label={s.keyLabel} />
            <span>to talk · click for hands-free</span>
          </FitRow>
        )}
        {(listening || processing) && (
          <div className="fade-in" style={{ transform: `translateX(${s.handsFree ? -11 : 0}px)` }}>
            <LevelBars
              levels={s.levels}
              listening={listening}
              processing={processing}
              count={13}
              mirrored
              maxHeight={24}
            />
          </div>
        )}
        {listening && s.handsFree && (
          <div className="pill-stop fade-in">
            <StopSquare size={10} radius={3} />
          </div>
        )}
        {s.phase === "done" && (
          <div className="pop">
            <CheckIcon size={14} color="var(--ok)" />
          </div>
        )}
        {s.phase === "failed" && (
          <div className="pop">
            <ExclamationIcon size={14} color="var(--amber)" />
          </div>
        )}
      </div>
    </div>
  );
}
