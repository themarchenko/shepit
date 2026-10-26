#!/usr/bin/env python3
"""Print one cost line for a Claude Code session (main transcript plus subagents).

    python3 scripts/agent/session-cost.py            newest session in this project
    python3 scripts/agent/session-cost.py <uuid>     a specific session id

Prices are estimates and drift; update PRICES when Anthropic does. A model that
is not in the table is listed as unpriced instead of being counted as free.
"""
import json
import os
import sys
from pathlib import Path

# $ per MTok: input, output, cache read, cache write 5m, cache write 1h
PRICES = {
    "claude-fable-5": (10, 50, 1, 12.5, 20),
    "claude-opus-5": (5, 25, 0.5, 6.25, 10),
    "claude-opus-4": (15, 75, 1.5, 18.75, 30),
    "claude-sonnet-5": (3, 15, 0.3, 3.75, 6),
    "claude-sonnet-4": (3, 15, 0.3, 3.75, 6),
    "claude-haiku-4-5": (1, 5, 0.1, 1.25, 2),
}
M = 1_000_000


def price_for(model):
    for key in sorted(PRICES, key=len, reverse=True):
        if model.startswith(key):
            return key, PRICES[key]
    return None


def main():
    # Claude Code names the project folder after the cwd with every non-alphanumeric char as '-'.
    slug = "".join(c if c.isalnum() else "-" for c in os.getcwd())
    proj = Path.home() / ".claude" / "projects" / slug
    if not proj.is_dir():
        print(f"session cost: no transcripts under {proj} (nothing to price)")
        return 0
    runs = sorted(proj.glob("*.jsonl"), key=lambda p: p.stat().st_mtime, reverse=True)
    if not runs:
        print(f"session cost: no session transcripts under {proj}")
        return 0
    session = sys.argv[1] if len(sys.argv) > 1 else runs[0].stem

    files = [proj / f"{session}.jsonl"]
    files += sorted((proj / session / "subagents").glob("*.jsonl"))

    per_model = {}
    for f in files:
        if not f.is_file():
            continue
        seen = {}  # streaming can write several rows per API call; keep one per message id
        with f.open(encoding="utf-8", errors="replace") as fh:
            for n, line in enumerate(fh):
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                m = d.get("message") if isinstance(d, dict) else None
                if not isinstance(m, dict) or not isinstance(m.get("usage"), dict):
                    continue
                seen[m.get("id") or d.get("requestId") or f"{f.name}:{n}"] = (m.get("model") or "?", m["usage"])
        for model, u in seen.values():
            t = per_model.setdefault(model, dict(i=0, o=0, cr=0, w5=0, w1=0, calls=0))
            t["calls"] += 1
            t["i"] += u.get("input_tokens") or 0
            t["o"] += u.get("output_tokens") or 0
            t["cr"] += u.get("cache_read_input_tokens") or 0
            cc = u.get("cache_creation") or {}
            w5 = cc.get("ephemeral_5m_input_tokens") or 0
            w1 = cc.get("ephemeral_1h_input_tokens") or 0
            total = u.get("cache_creation_input_tokens") or 0
            t["w5"] += w5 + max(0, total - w5 - w1)  # unattributed writes count at the 5m rate
            t["w1"] += w1

    total_cost = 0.0
    calls = out = cw = cr = 0
    parts, unpriced = [], []
    for model, t in per_model.items():
        pr = price_for(model)
        if model.startswith("<"):  # "<synthetic>" rows carry no billed usage
            continue
        if not pr:
            unpriced.append(model)
            continue
        key, (pi, po, pcr, p5, p1) = pr
        cost = (t["i"] * pi + t["o"] * po + t["cr"] * pcr + t["w5"] * p5 + t["w1"] * p1) / M
        total_cost += cost
        calls += t["calls"]
        out += t["o"]
        cw += t["w5"] + t["w1"]
        cr += t["cr"]
        parts.append(f"{key.replace('claude-', '')} ${cost:.2f}")

    def fmt(n):
        return f"{n / M:.1f}M" if n >= M else f"{round(n / 1000)}k"

    line = (
        f"session {session[:8]}: {calls} calls | out {fmt(out)} | cacheW {fmt(cw)} | "
        f"cacheR {fmt(cr)} -> est ${total_cost:.2f}"
    )
    if parts:
        line += f" ({', '.join(parts)})"
    if unpriced:
        line += f" | unpriced: {', '.join(sorted(unpriced))}"
    print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
