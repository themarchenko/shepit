import { useState } from "react";
import { checkMicrophone, openMicrophonePrivacy, type MicCheck } from "./api";
import { Button } from "./controls";

export const ADMIN_NOTE =
  "Apps running as administrator: Windows does not let Shepit paste into them. The text is copied instead, so press Ctrl+V.";

type Status = { kind: "idle" } | { kind: "testing" } | { kind: "done"; check: MicCheck };

// The two Windows switches that block a microphone, in the order the user meets them.
function PrivacyGuidance({ lead, onTestAgain }: { lead: string; onTestAgain: () => void }) {
  return (
    <div className="sp-warn" role="status">
      <p style={{ margin: "0 0 6px" }}>{lead}</p>
      <p style={{ margin: "0 0 8px" }}>
        In Windows Settings, open Privacy &amp; security, then Microphone. Turn on &quot;Microphone access&quot; and
        &quot;Let desktop apps access your microphone&quot;.
      </p>
      <div className="sp-mic-actions">
        <Button onClick={() => openMicrophonePrivacy().catch(() => {})}>Open privacy settings</Button>
        <Button onClick={onTestAgain}>Test again</Button>
      </div>
    </div>
  );
}

// A one second test of the default microphone, started only by the user so the Windows microphone
// indicator never lights up unasked. Nothing it hears is kept.
export default function MicrophoneCheck() {
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  const run = () => {
    setStatus({ kind: "testing" });
    checkMicrophone()
      .then((check) => setStatus({ kind: "done", check }))
      .catch((e: unknown) => setStatus({ kind: "done", check: { result: "failed", detail: String(e) } }));
  };

  const testing = status.kind === "testing";
  const check = status.kind === "done" ? status.check : null;
  // The guidance has its own "Test again" button.
  const guided = check?.result === "silent" || check?.result === "denied";

  return (
    <div className="sp-block">
      <div className="sp-block-head">
        <div className="sp-row-text">
          <span>Microphone</span>
          <span className="sp-row-detail">
            {testing ? "Listening for one second. Say something." : "Listens for one second. Nothing is saved."}
          </span>
        </div>
        <div className="sp-row-control">
          {check?.result === "ok" && (
            <span className="sp-status sp-ok">
              <i className="sp-dot ok" />
              Microphone works
            </span>
          )}
          {!guided && (
            <Button data-mic-test disabled={testing} onClick={run}>
              {testing ? "Testing" : check ? "Test again" : "Test microphone"}
            </Button>
          )}
        </div>
      </div>
      {guided && check?.result === "silent" && (
        <PrivacyGuidance lead="The microphone opened but Shepit heard no sound." onTestAgain={run} />
      )}
      {guided && check?.result === "denied" && (
        <PrivacyGuidance lead="Windows is not letting Shepit use the microphone." onTestAgain={run} />
      )}
      {check?.result === "noDevice" && (
        <div className="sp-warn" role="status">
          No microphone found. Plug one in, or choose a default input device in the Windows sound settings.
        </div>
      )}
      {check?.result === "failed" && (
        <div className="sp-warn" role="status">
          The microphone could not be opened{check.detail ? `: ${check.detail}` : "."}
        </div>
      )}
      <p className="sp-note">{ADMIN_NOTE}</p>
    </div>
  );
}
