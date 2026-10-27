import { useEffect, useState } from "react";

interface Props {
  levels: number[];
  listening: boolean;
  processing: boolean;
  count: number;
  mirrored: boolean;
  maxHeight: number;
  width?: number;
  spacing?: number;
}

// WidgetView.swift LevelBars. Mirrored around the middle for the pill, newest on the right otherwise.
// While processing the bars become a slow travelling wave.
export default function LevelBars({
  levels,
  listening,
  processing,
  count,
  mirrored,
  maxHeight,
  width = 3,
  spacing = 3,
}: Props) {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (!processing) return;
    const id = setInterval(() => setT(performance.now() / 1000), 33);
    return () => clearInterval(id);
  }, [processing]);

  const level = (ago: number) => {
    const v = levels[Math.max(0, levels.length - 1 - ago)] ?? 0;
    return Math.min(1, Math.max(0, v));
  };
  const mid = (count - 1) / 2;
  const wave = (i: number) => 0.5 + 0.5 * Math.sin(t * 7 - i * 0.7);

  const height = (i: number) => {
    if (processing) return 3 + 3.5 * wave(i);
    if (!listening) return 3;
    const d = Math.abs(i - mid);
    const ago = mirrored ? Math.round(d * 2) : (count - 1 - i) * 2;
    const shrink = mirrored ? 1 - d * 0.06 : 1 - d * 0.1;
    return 3 + level(ago) * (maxHeight - 3) * shrink;
  };

  return (
    <div className="bars" style={{ height: maxHeight, gap: spacing }}>
      {Array.from({ length: count }, (_, i) => (
        <span
          key={i}
          className="bar"
          style={{ width, height: height(i), opacity: processing ? 0.3 + 0.7 * wave(i) : 1 }}
        />
      ))}
    </div>
  );
}
