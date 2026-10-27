import { liveFull, processingLabel, stepOf } from "./captionContent";
import { CheckIcon, ExclamationIcon, LockIcon, MicIcon } from "./Icons";
import FitRow from "./FitRow";
import KeyCap from "./KeyCap";
import LevelBars from "./LevelBars";
import StepBar from "./StepBar";
import { glassStyle, shapeStyle, type StyleProps } from "./styleProps";

// 4. One shape that morphs from a compact status line into a card with two lines of text and the steps.
export default function Island({ s, elapsed }: StyleProps) {
  const listening = s.phase === "listening";
  const hasLive = Boolean(s.live.stable || s.live.unstable);
  const showsBody =
    (listening && hasLive) || s.phase === "processing" || s.phase === "done";

  return (
    <div className="glass shape island" style={{ ...shapeStyle(s), ...glassStyle(s) }}>
      <div
        className={s.phase === "idle" ? "island-head compact fade-in" : "island-head fade-in"}
        style={{ height: Math.min(36, s.shape.height) }}
        key={s.phase}
      >
        {s.phase === "idle" && (
          <>
            {s.hovering ? (
              <FitRow size={12.5}>
                <span>Hold</span>
                <KeyCap label={s.keyLabel} />
                <span>· click for hands-free</span>
              </FitRow>
            ) : (
              <>
                <MicIcon size={10} />
                <KeyCap label={s.keyLabel} />
              </>
            )}
          </>
        )}
        {listening && (
          <>
            <span className="rec-dot" />
            <span>{s.handsFree ? "Hands-free" : "Listening"}</span>
            <span className="grow" />
            {s.handsFree && <LockIcon size={9} />}
            <LevelBars
              levels={s.levels}
              listening
              processing={false}
              count={9}
              mirrored={false}
              maxHeight={16}
              width={2.5}
              spacing={2}
            />
            <span className="mono-digits island-time">{elapsed}</span>
          </>
        )}
        {s.phase === "processing" && <span>{processingLabel(s)}</span>}
        {s.phase === "done" && (
          <>
            <CheckIcon size={11} color="var(--ok)" />
            <span>{s.target ? `Inserted into ${s.target}` : "Inserted"}</span>
          </>
        )}
        {s.phase === "failed" && (
          <>
            <ExclamationIcon size={12} color="var(--amber)" />
            <span className="amber island-fail">{s.message}</span>
          </>
        )}
      </div>
      {showsBody && (
        <div className="island-body fade-in">
          {s.phase === "done" ? (
            <p className="island-clamp">{s.message}</p>
          ) : s.phase === "processing" ? (
            <p className="island-tail dim">
              <span>{liveFull(s)}</span>
            </p>
          ) : (
            <p className="island-tail">
              <span>
                {s.live.stable}
                <span className="dim">{(s.live.stable ? " " : "") + s.live.unstable}</span>
              </span>
            </p>
          )}
        </div>
      )}
      <span className="grow" />
      {s.phase === "processing" && (
        <div className="island-steps fade-in">
          <StepBar step={stepOf(s.stage)} labels={["Transcribe", "Format", "Insert"]} height={22} />
        </div>
      )}
    </div>
  );
}
