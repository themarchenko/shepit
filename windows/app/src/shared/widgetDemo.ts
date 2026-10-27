import { useEffect, useState } from "react";
import { formatElapsed } from "../captionContent";
import type { WidgetState, WidgetStyle } from "../widgetState";
import { widgetDemo, type DemoStep } from "./api";

// WidgetDemo.swift startLevels: syllables at about 4 Hz with a breath every few seconds.
export function voiceLevel(t: number, noise: number): number {
  const phrase = t % 2.7 > 2.3 ? 0.1 : 1;
  const syllable = Math.pow(0.5 + 0.5 * Math.sin(t * 26 + Math.sin(t * 3.1) * 2.2), 1.5);
  return Math.min(1, phrase * (0.16 + 0.84 * syllable * (0.6 + 0.4 * Math.sin(t * 1.3 + 1))) + noise * 0.07);
}

export interface DemoView {
  steps: DemoStep[];
  // The state to draw; null until the script has arrived.
  state: WidgetState | null;
  // Index into `steps`.
  index: number;
  elapsed: string;
}

const LEVELS = 32;
const FRAME_MS = 40;

// Loops the script Rust built with the real layout. Rust owns the sizes and timings; this only runs
// the clock and animates the bars, which the script leaves flat.
export function useDemo(style: WidgetStyle, hotkeyId: string, glassOpacity: number): DemoView {
  const [steps, setSteps] = useState<DemoStep[]>([]);
  const [index, setIndex] = useState(0);
  const [levels, setLevels] = useState<number[]>(() => new Array(LEVELS).fill(0));
  const [elapsedMs, setElapsedMs] = useState(0);

  useEffect(() => {
    let alive = true;
    widgetDemo(style)
      .then((s) => {
        if (!alive) return;
        setSteps(s);
        setIndex(0);
      })
      .catch((e: unknown) => console.warn("widget_demo failed", e));
    return () => {
      alive = false;
    };
    // The key label is part of the script, so a new key fetches it again.
  }, [style, hotkeyId]);

  const step = steps.length ? steps[index % steps.length] : null;

  useEffect(() => {
    if (!step) return;
    const id = window.setTimeout(() => setIndex((i) => (i + 1) % steps.length), step.seconds * 1000);
    return () => window.clearTimeout(id);
  }, [step, steps.length]);

  const listening = step?.state.phase === "listening";
  useEffect(() => {
    if (!listening) return;
    const started = performance.now();
    const id = window.setInterval(() => {
      const t = (performance.now() - started) / 1000;
      setElapsedMs(performance.now() - started);
      setLevels((prev) => [...prev.slice(1), voiceLevel(t, Math.random())]);
    }, FRAME_MS);
    return () => {
      window.clearInterval(id);
      setElapsedMs(0);
      setLevels(new Array(LEVELS).fill(0));
    };
  }, [listening]);

  const state = step ? { ...step.state, levels, glassOpacity } : null;
  return { steps, state, index, elapsed: formatElapsed(elapsedMs) };
}
