import { useCallback, useEffect, useRef, useState } from "react";
import { historyClear, historyRecent, onHistoryChanged, type HistoryItem, type HistoryPayload } from "../shared/api";
import { PrimaryButton, Button } from "../shared/controls";
import { useEvent } from "../shared/useSettings";

// "14:05", plus the date when the row is not from today ("Oct 26, 14:05"; the year only when it is not this one).
export function whenText(ms: number, now = new Date()): string {
  const d = new Date(ms);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return time;
  const date = d.toLocaleDateString([], d.getFullYear() === now.getFullYear() ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
  return `${date}, ${time}`;
}

function CopyIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15V6a2 2 0 0 1 2-2h8" />
    </svg>
  );
}

type CopyNote = "copied" | "manual" | null;

function Row({ item }: { item: HistoryItem }) {
  const [note, setNote] = useState<CopyNote>(null);
  const formatted = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    let next: CopyNote = "copied";
    try {
      await navigator.clipboard.writeText(item.formattedText);
    } catch {
      // No clipboard access: select the text so Ctrl+C works.
      const el = formatted.current;
      if (el) {
        const range = document.createRange();
        range.selectNodeContents(el);
        const sel = window.getSelection();
        sel?.removeAllRanges();
        sel?.addRange(range);
      }
      next = "manual";
    }
    setNote(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setNote(null), next === "copied" ? 1500 : 4000);
  };

  return (
    <li className="hi-row" data-id={item.id}>
      <div className="hi-head">
        <span className="hi-time">{whenText(item.createdAtMs)}</span>
        <span className="hi-app">{item.appName ?? item.appId ?? "?"}</span>
        <span className="hi-meta">{`${item.profile} · ${item.mode} · ${item.formatterSource}`}</span>
        <span className="hi-meta">{`stt ${item.sttMs} ms · llm ${item.llmMs} ms`}</span>
        <span className="hi-spacer" />
        {note && <span className="hi-copied" role="status">{note === "copied" ? "Copied" : "Press Ctrl+C"}</span>}
        <button type="button" className="hi-copy" aria-label="Copy the formatted text" title="Copy" onClick={copy}>
          <CopyIcon />
        </button>
      </div>
      <div className="hi-raw">{item.rawText}</div>
      <div className="hi-formatted" ref={formatted}>{item.formattedText}</div>
    </li>
  );
}

const countText = (n: number) => `${n} ${n === 1 ? "dictation" : "dictations"}`;

export default function HistoryApp() {
  const [data, setData] = useState<HistoryPayload | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [clearError, setClearError] = useState<string | null>(null);

  const reload = useCallback(() => {
    historyRecent()
      .then((d) => {
        setData(d);
        setFailed(null);
      })
      .catch((e) => setFailed(String(e)));
  }, []);

  useEffect(reload, [reload]);
  useEvent(() => onHistoryChanged(reload));

  const clear = async () => {
    setConfirming(false);
    try {
      await historyClear();
      setClearError(null);
    } catch (e) {
      setClearError(`Could not clear history: ${String(e)}`);
    }
    reload();
  };

  if (!data && !failed) return null;
  const items = data?.items ?? [];
  const total = data?.total ?? 0;
  const error = data?.error ?? failed;

  return (
    <div className="hi-window">
      <div className="hi-bar">
        {confirming ? (
          <>
            <span className="hi-confirm" role="alertdialog" aria-label="Clear history">
              {`Delete all ${total} ${total === 1 ? "dictation" : "dictations"} and their recordings? This cannot be undone.`}
            </span>
            <span className="hi-spacer" />
            <Button data-cancel onClick={() => setConfirming(false)}>Cancel</Button>
            <PrimaryButton data-delete onClick={clear}>Delete</PrimaryButton>
          </>
        ) : (
          <>
            <span className="hi-count" data-count>
              {error ? "" : total > items.length ? `Showing the newest ${items.length} of ${total}` : countText(items.length)}
            </span>
            <span className="hi-spacer" />
            <Button data-clear disabled={total === 0 && !error} onClick={() => setConfirming(true)}>Clear history…</Button>
          </>
        )}
      </div>
      {error && <p className="hi-error" role="alert">{error}</p>}
      {clearError && <p className="hi-error" role="alert">{clearError}</p>}
      {items.length === 0 && !error ? (
        <p className="hi-empty">No dictations yet.</p>
      ) : (
        <ul className="hi-list">
          {items.map((it) => (
            <Row key={it.id} item={it} />
          ))}
        </ul>
      )}
    </div>
  );
}
