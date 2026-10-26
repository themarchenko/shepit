#!/usr/bin/env python3
"""Drive a persistent headless Claude Code worker and measure per-turn latency.

Sends one dictation at a time (waits for the result before the next), the way the
app will. Usage: python3 apple/spike/worker/probe.py [--model haiku] [--schema]
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA = {
    "type": "object",
    "properties": {
        "text": {"type": "string"},
        "language": {"type": "string", "enum": ["uk", "en", "mixed"]},
        "new_terms": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["text", "language", "new_terms"],
    "additionalProperties": False,
}

CASES = [
    ("clean", "ем так зроби будь ласка рефакторинг функції хендл сабміт у файлі юзер сервіс свіфт і винеси ну валідацію в окремий метод валідейт інпут"),
    ("prompt", "окей мені треба щоб ти проаналізував поточну архітектуру застосунку віспр зокрема ну модуль вставки тексту через ексесібіліті апі і запропонував як зробити фолбек на пейст через сі джі івент якщо ей екс селектед текст не спрацював поясни ризики з буфером обміну і ще додай план тестування у терміналі хромі і телеграмі"),
    ("clean", "send the invoice on monday actually no on wednesday after the demo is confirmed and um cc the finance team"),
    ("clean", "запусти гіт рібейз на мейн потім енпіем ран білд і подивись чи не падає тест на сі ай"),
]


def user_message(mode, transcript):
    ctx = {
        "app": "Terminal",
        "bundle_id": "com.apple.Terminal",
        "window_title": "Claude Code - shepit",
        "mode": mode,
        "language_hint": "auto",
        "text_before_caret": "",
    }
    body = "Context:\n" + json.dumps(ctx, ensure_ascii=False) + "\n\nTranscript:\n" + transcript
    return {"type": "user", "message": {"role": "user", "content": body}}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="haiku")
    ap.add_argument("--schema", action="store_true", help="pass --json-schema")
    args = ap.parse_args()

    env = dict(os.environ)
    for k in ("CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT"):
        env.pop(k, None)
    cmd = [
        "claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
        "--verbose", "--model", args.model, "--tools", "", "--setting-sources", "",
        "--strict-mcp-config", "--no-session-persistence",
        "--system-prompt-file", os.path.join(HERE, "system-prompt.md"),
    ]
    if args.schema:
        cmd += ["--json-schema", json.dumps(SCHEMA)]

    workdir = os.path.join(HERE, ".workdir")
    os.makedirs(workdir, exist_ok=True)
    t_start = time.monotonic()
    proc = subprocess.Popen(cmd, cwd=workdir, env=env, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

    def read_until_result():
        while True:
            line = proc.stdout.readline()
            if not line:
                err = proc.stderr.read()
                raise SystemExit("worker exited early: " + err[-800:])
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            if ev.get("type") == "system" and ev.get("subtype") == "init":
                print("init after %.2f s; tools=%s mcp=%s model=%s" % (
                    time.monotonic() - t_start, ev.get("tools"), ev.get("mcp_servers"), ev.get("model")))
            if ev.get("type") == "result":
                return ev

    rows = []
    for i, (mode, transcript) in enumerate(CASES, 1):
        t0 = time.monotonic()
        proc.stdin.write(json.dumps(user_message(mode, transcript), ensure_ascii=False) + "\n")
        proc.stdin.flush()
        res = read_until_result()
        wall = time.monotonic() - t0
        usage = res.get("usage", {})
        structured = res.get("structured_output")
        text = structured["text"] if structured else res.get("result", "")
        rows.append((i, mode, wall, res.get("duration_api_ms"), usage.get("input_tokens"),
                     usage.get("cache_creation_input_tokens"), usage.get("cache_read_input_tokens"),
                     usage.get("output_tokens"), structured is not None))
        print("\n[%d] mode=%s wall=%.2fs api=%sms in=%s cache_create=%s cache_read=%s out=%s structured=%s" % rows[-1])
        print("    result: " + (json.dumps(structured, ensure_ascii=False) if structured else text)[:600])

    proc.stdin.close()
    try:
        proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()
    print("\nsummary: median wall %.2f s over %d turns" % (sorted(r[2] for r in rows)[len(rows) // 2], len(rows)))


if __name__ == "__main__":
    main()
