import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { onUpdateCard, updateAction, updateCardReady, updateResize, type CardButton, type CardPayload } from "../shared/api";
import { useEvent } from "../shared/useSettings";

// The 20 px of transparent window around the 380 px card (the macOS panel pads the same way).
const MARGIN = 20;

function Mark() {
  return (
    <svg className="up-mark" width="16" height="16" viewBox="0 0 34 34" fill="currentColor" aria-hidden="true">
      {[6, 12, 20, 14, 8].map((h, i) => (
        <rect key={i} x={4 + i * 6} y={17 - h / 2} width="3.4" height={h} rx="1.7" />
      ))}
    </svg>
  );
}

function Action({ button }: { button: CardButton }) {
  return (
    <button type="button" className={`sp-btn ${button.kind}`} data-action={button.id} onClick={() => updateAction(button.id).catch(() => {})}>
      {button.label}
    </button>
  );
}

// Draws exactly what Rust sends: the texts and the buttons are decided there (updater.rs card_for).
export default function UpdateCard() {
  const [card, setCard] = useState<CardPayload | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    updateCardReady()
      .then((c) => alive && setCard(c))
      .catch((e) => console.warn("update_card_ready failed", e));
    return () => {
      alive = false;
    };
  }, []);
  useEvent(() => onUpdateCard(setCard));

  // The window is shown by the first resize, so it never appears at the wrong height.
  useLayoutEffect(() => {
    if (!card || !ref.current) return;
    updateResize(Math.ceil(ref.current.offsetHeight) + 2 * MARGIN).catch(() => {});
  }, [card]);

  if (!card) return null;
  const lead = card.buttons.filter((b) => b.lead);
  const rest = card.buttons.filter((b) => !b.lead);
  return (
    <div className="up-window">
      <div className="up-card" ref={ref} role="dialog" aria-label={card.title}>
        <div className="up-title">
          <Mark />
          <span>{card.title}</span>
        </div>
        {card.detail && <p className="up-detail">{card.detail}</p>}
        {card.progress && (
          <div
            className={card.progress.fraction === null ? "up-bar indeterminate" : "up-bar"}
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={card.progress.fraction === null ? undefined : Math.round(card.progress.fraction * 100)}
          >
            <i style={card.progress.fraction === null ? undefined : { width: `${Math.round(card.progress.fraction * 100)}%` }} />
          </div>
        )}
        {card.link && (
          <button type="button" className="up-link" data-action={card.link.id} onClick={() => updateAction(card.link!.id).catch(() => {})}>
            {card.link.label}
          </button>
        )}
        {card.buttons.length > 0 && (
          <div className="up-buttons">
            <div className="up-lead">{lead.map((b) => <Action key={b.id} button={b} />)}</div>
            <div className="up-rest">{rest.map((b) => <Action key={b.id} button={b} />)}</div>
          </div>
        )}
      </div>
    </div>
  );
}
