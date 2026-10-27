import { useEffect, useState } from "react";
import { checkUpdatesNow, getUpdateStatus, onUpdateStatus, openRecordingsFolder, showTour, showUpdateCard, type UpdateStatus } from "../shared/api";
import { Button, Group, Row, Toggle } from "../shared/controls";
import { type PaneProps } from "../shared/useSettings";

const RELATIVE = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

// "Checked yesterday", "Checked 3 hours ago". `lastCheck` is unix seconds.
export function checkedText(lastCheck: number, nowSeconds: number): string {
  const ago = Math.max(0, nowSeconds - lastCheck);
  if (ago < 60) return `Checked ${RELATIVE.format(0, "minute")}`;
  if (ago < 3600) return `Checked ${RELATIVE.format(-Math.floor(ago / 60), "minute")}`;
  if (ago < 86400) return `Checked ${RELATIVE.format(-Math.floor(ago / 3600), "hour")}`;
  return `Checked ${RELATIVE.format(-Math.floor(ago / 86400), "day")}`;
}

function detailOf(s: UpdateStatus): string {
  if (s.message) return s.message;
  if (s.lastCheck === null) return "Not checked yet";
  return checkedText(s.lastCheck, Date.now() / 1000);
}

const BUSY = new Set(["checking", "downloading", "installing"]);

export default function AboutPane({ settings, update }: PaneProps) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  // Listen first, then ask: an event between the answer and the listener would be lost.
  useEffect(() => {
    let alive = true;
    let off: (() => void) | undefined;
    onUpdateStatus(setStatus)
      .then((fn) => {
        if (!alive) return fn();
        off = fn;
        return getUpdateStatus().then((s) => alive && setStatus(s));
      })
      .catch(() => {});
    return () => {
      alive = false;
      off?.();
    };
  }, []);

  return (
    <>
      <Group>
        <div className="sp-about">
          <div className="sp-appicon" aria-hidden="true">
            <svg width="34" height="34" viewBox="0 0 34 34" fill="#f4f2ee">
              {[6, 12, 20, 14, 8].map((h, i) => (
                <rect key={i} x={4 + i * 6} y={17 - h / 2} width="3.4" height={h} rx="1.7" />
              ))}
            </svg>
          </div>
          <div>
            <div className="sp-about-title">Shepit {settings.version}</div>
            <p className="sp-note">Speech recognition runs on this PC. Your audio never leaves it.</p>
          </div>
        </div>
      </Group>
      <Group title="Updates">
        <Row label="Check for updates once a day">
          <Toggle
            checked={settings.checkForUpdates}
            onChange={(checkForUpdates) => update({ checkForUpdates })}
            label="Check for updates once a day"
          />
        </Row>
        {status && (
          <Row label={status.label} detail={detailOf(status)}>
            {status.state === "available" ? (
              <Button onClick={() => showUpdateCard().catch(() => {})}>Install…</Button>
            ) : BUSY.has(status.state) ? (
              <span className="sp-spinner" role="progressbar" aria-label="Working" />
            ) : (
              <Button onClick={() => checkUpdatesNow().catch(() => {})}>Check now</Button>
            )}
          </Row>
        )}
      </Group>
      <Group>
        <Row label="Welcome tour">
          <Button onClick={() => showTour().catch(() => {})}>Show again</Button>
        </Row>
        <Row label="Recordings" detail="Last 200 dictations, kept for comparing recognizers">
          <Button onClick={() => openRecordingsFolder().catch(() => {})}>Open folder</Button>
        </Row>
      </Group>
      <Group title="Credits">
        <div className="sp-block">
          <p className="sp-note">
            Speech model: NVIDIA Parakeet v3, repackaged for ONNX by istupakov. Licensed under CC-BY-4.0
            (creativecommons.org/licenses/by/4.0).
          </p>
        </div>
      </Group>
    </>
  );
}
