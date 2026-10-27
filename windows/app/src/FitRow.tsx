import { useEffect, useRef, type ReactNode } from "react";

const MIN_SIZE = 10;

// A centred row that never clips mid-glyph: if it is wider than the room it has, the font shrinks in
// half-pixel steps down to 10 px, whatever the font. Imperative on purpose: React state would re-render
// on every measurement, and the result only depends on the DOM widths.
export default function FitRow({ children, size, className }: { children: ReactNode; size: number; className?: string }) {
  const outer = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const o = outer.current;
    const i = inner.current;
    if (!o || !i) return;
    const fit = () => {
      let px = size;
      i.style.fontSize = `${px}px`;
      while (i.scrollWidth > o.clientWidth + 0.5 && px > MIN_SIZE) {
        px -= 0.5;
        i.style.fontSize = `${px}px`;
      }
    };
    fit();
    // Only the outer box is observed: resizing the inner row would otherwise retrigger the fit.
    const ro = new ResizeObserver(fit);
    ro.observe(o);
    return () => ro.disconnect();
  }, [size, children]);

  return (
    <div ref={outer} className={className ? `fit ${className}` : "fit"}>
      <div ref={inner} className="fit-inner" style={{ fontSize: size }}>
        {children}
      </div>
    </div>
  );
}
