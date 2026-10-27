import { useEffect, useState } from "react";
import { detectAgents, type Polishing } from "./api";
import type { PaneProps } from "./useSettings";

interface Found {
  claude: string | null;
  codex: string | null;
}

const NAMES: Record<Polishing, string> = { claude: "Claude Code", codex: "Codex", off: "Off" };

// Claude Code, Codex or nothing, with what was found on this PC. A missing agent cannot be picked, and
// when the chosen one is gone, `detect_agents` already moved the choice to the other agent or to off
// (SharedControls.swift ProviderList). Rust re-sends the settings, so this list only reads the paths.
export default function ProviderList({ settings, update }: PaneProps) {
  const [found, setFound] = useState<Found | null>(null);

  useEffect(() => {
    let alive = true;
    detectAgents()
      .then((r) => alive && setFound({ claude: r.claude, codex: r.codex }))
      .catch((e: unknown) => console.warn("detect_agents failed", e));
    return () => {
      alive = false;
    };
  }, []);

  const detail = (id: Polishing, path: string | null): string => {
    if (id === "claude") return path ? `Uses your Claude subscription. ${path}` : "Install Claude Code from claude.ai/code, then run claude once to sign in.";
    if (id === "codex") return path ? `Uses your ChatGPT plan. ${path}` : "Install with npm i -g @openai/codex, then run codex once to sign in.";
    return "Insert what you said. Spacing, capitals and dictionary terms are still fixed.";
  };

  return (
    <div className="sp-providers" role="radiogroup" aria-label="Polishing">
      {found && !found.claude && !found.codex && (
        <div className="sp-warn" data-no-agent>
          No AI agent found on this PC, so Shepit inserts the text as heard. Install Claude Code or Codex and it shows up here.
        </div>
      )}
      {(["claude", "codex", "off"] as const).map((id) => {
        const path = id === "off" ? null : (found?.[id] ?? null);
        const available = id === "off" || path !== null;
        const selected = settings.polishing === id;
        return (
          <button
            key={id}
            type="button"
            role="radio"
            aria-checked={selected}
            className="sp-provider"
            data-provider={id}
            disabled={!available}
            onClick={() => void update({ polishing: id })}
          >
            <span className="sp-radio" />
            <span className="sp-provider-text">
              <span className="sp-provider-head">
                {NAMES[id]}
                {id !== "off" && (
                  <span className="sp-provider-state">
                    <i className={available ? "sp-dot ok" : "sp-dot"} />
                    {available ? "Found" : "Not installed"}
                  </span>
                )}
              </span>
              <span className="sp-provider-detail">{detail(id, path)}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
