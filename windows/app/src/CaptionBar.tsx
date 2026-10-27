import { captionContent, processingLabel, stepOf } from "./captionContent";
import { CheckIcon, ExclamationIcon, StopSquare } from "./Icons";
import FitRow from "./FitRow";
import KeyCap from "./KeyCap";
import LevelBars from "./LevelBars";
import LiveLine from "./LiveLine";
import StepBar from "./StepBar";
import { glassStyle, shapeStyle, type StyleProps } from "./styleProps";

// 2. A wide bar built around the words, like subtitles, with the target app on the right.
export default function CaptionBar({ s, elapsed }: StyleProps) {
  const listening = s.phase === "listening";
  const processing = s.phase === "processing";
  const content = captionContent(s);

  return (
    <div className="glass shape" style={{ ...shapeStyle(s), ...glassStyle(s) }}>
      {s.phase === "idle" ? (
        <FitRow size={12.5} className="bar-idle fade-in">
          <span>Shepit</span>
          <KeyCap label={s.keyLabel} />
          {s.hovering && <span>hold to talk · click for hands-free</span>}
        </FitRow>
      ) : (
        <div className="bar-row fade-in" key="active">
          <div className="bar-badge">
            {s.phase === "done" ? (
              <CheckIcon size={12} color="var(--ok)" />
            ) : s.phase === "failed" ? (
              <ExclamationIcon size={13} color="var(--amber)" />
            ) : (
              <LevelBars
                levels={s.levels}
                listening={listening}
                processing={processing}
                count={5}
                mirrored={false}
                maxHeight={18}
                width={2.5}
                spacing={2}
              />
            )}
          </div>
          <div className="bar-text">
            {content && <LiveLine {...content} size={14} />}
          </div>
          <div className="bar-meta">
            {listening && <span className="mono-digits">{elapsed}</span>}
            {processing && <span>{processingLabel(s).toLowerCase()}</span>}
            {s.phase === "done" && <span>inserted</span>}
            {s.target && s.phase !== "failed" && <span className="chip">{s.target}</span>}
            {listening && s.handsFree && (
              <span className="bar-lock">
                <StopSquare size={8} radius={2} />
              </span>
            )}
          </div>
        </div>
      )}
      {processing && (
        <div className="bar-steps fade-in">
          <StepBar step={stepOf(s.stage)} />
        </div>
      )}
    </div>
  );
}
