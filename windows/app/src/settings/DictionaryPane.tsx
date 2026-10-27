import { useEffect, useRef, useState } from "react";
import { Group } from "../shared/controls";
import { type PaneProps } from "../shared/useSettings";

// Same rule as dictionary::normalize_terms in Rust: split on any line break, trim, drop blanks. Duplicates stay.
export function termsIn(text: string): string[] {
  return text
    .split(/\r\n|\r|\n/)
    .map((t) => t.trim())
    .filter((t) => t !== "");
}

const same = (a: string[], b: string[]) => a.length === b.length && a.every((t, i) => t === b[i]);

export default function DictionaryPane({ settings, update }: PaneProps) {
  const [text, setText] = useState(() => settings.dictionary.join("\n"));
  // At most one patch in flight; typing meanwhile only replaces the queued list.
  const inFlight = useRef(false);
  const queued = useRef<string[] | null>(null);
  // Bumped when the queue drains, so the saved list is compared with the text once more.
  const [settled, setSettled] = useState(0);

  const send = async (terms: string[]) => {
    if (inFlight.current) {
      queued.current = terms;
      return;
    }
    inFlight.current = true;
    let next: string[] | null = terms;
    while (next) {
      queued.current = null;
      await update({ dictionary: next });
      next = queued.current;
    }
    inFlight.current = false;
    setSettled((n) => n + 1);
  };

  // Learned terms arrive while the pane is open. The text is replaced only when its terms differ from the
  // incoming list, so typing a new line is not undone; while a patch is out, the list is stale on purpose.
  useEffect(() => {
    if (inFlight.current) return;
    setText((current) => (same(termsIn(current), settings.dictionary) ? current : settings.dictionary.join("\n")));
  }, [settings.dictionary, settled]);

  return (
    <Group title="Terms">
      <div className="sp-block">
        <textarea
          className="sp-input sp-terms"
          aria-label="Dictionary terms"
          spellCheck={false}
          value={text}
          onChange={(e) => {
            const v = e.currentTarget.value;
            setText(v);
            send(termsIn(v)).catch(() => {});
          }}
        />
        <p className="sp-note">
          One per line: names, products and identifiers the recognizer gets wrong. Terms the polishing agent fixes
          are added here.
        </p>
      </div>
    </Group>
  );
}
