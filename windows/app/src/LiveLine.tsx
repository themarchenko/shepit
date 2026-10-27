import { useEffect, useRef, useState } from "react";
import type { CaptionContent } from "./captionContent";

interface Props extends Omit<CaptionContent, "label"> {
  size?: number;
}

// One line. `start` keeps the newest words and fades the clipped head; `end` ends in an ellipsis.
export default function LiveLine({ stable, unstable, dim, tone, clip, size = 13 }: Props) {
  const outer = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLSpanElement>(null);
  const [over, setOver] = useState(false);

  useEffect(() => {
    const o = outer.current;
    const i = inner.current;
    if (!o || !i) return;
    const ro = new ResizeObserver(() => setOver(i.scrollWidth > o.clientWidth + 0.5));
    ro.observe(o);
    ro.observe(i);
    return () => ro.disconnect();
  }, []);

  const lead = dim ? "dim" : tone === "amber" ? "amber" : "";
  const cls = ["live-line", clip, clip === "start" && over ? "fade-head" : ""].filter(Boolean).join(" ");
  return (
    <div ref={outer} className={cls} style={{ fontSize: size }}>
      <span ref={inner} className="live-inner">
        <span className={lead}>{stable}</span>
        {unstable && <span className="dim">{(stable ? " " : "") + unstable}</span>}
      </span>
    </div>
  );
}
