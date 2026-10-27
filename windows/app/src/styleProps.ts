import type { CSSProperties } from "react";
import type { WidgetState } from "./widgetState";

export interface StyleProps {
  s: WidgetState;
  elapsed: string;
}

// The springing box: size and radius straight from the payload.
export function shapeStyle(s: WidgetState): CSSProperties {
  return { width: s.shape.width, height: s.shape.height, borderRadius: s.shape.radius };
}

export function glassStyle(s: WidgetState): CSSProperties {
  return { background: `rgba(0, 0, 0, ${s.glassOpacity})` };
}
