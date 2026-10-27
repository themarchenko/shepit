// Inline SVG: the widget has no icon font and the Windows app adds no dependencies.
interface IconProps {
  size?: number;
  color?: string;
}

function Svg({ size = 12, color = "currentColor", children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke={color}
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="icon"
    >
      {children}
    </svg>
  );
}

export const CheckIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M3 8.5 6.5 12 13 4.5" />
  </Svg>
);

export const ExclamationIcon = (p: IconProps) => (
  <Svg {...p}>
    <path d="M8 2.5v7" />
    <path d="M8 13.2v.1" />
  </Svg>
);

export const MicIcon = ({ size = 12, color = "currentColor" }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill={color} aria-hidden="true" className="icon">
    <rect x="5.5" y="1.5" width="5" height="8.5" rx="2.5" />
    <path
      d="M3.2 7.8a4.8 4.8 0 0 0 9.6 0M8 12.6v2"
      fill="none"
      stroke={color}
      strokeWidth="1.5"
      strokeLinecap="round"
    />
  </svg>
);

export const LockIcon = ({ size = 10, color = "currentColor" }: IconProps) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill={color} aria-hidden="true" className="icon">
    <rect x="3" y="7" width="10" height="7.5" rx="1.8" />
    <path d="M5.2 7V5a2.8 2.8 0 0 1 5.6 0v2" fill="none" stroke={color} strokeWidth="1.8" />
  </svg>
);

// The hands-free "stop" square.
export const StopSquare = ({ size, radius = 3 }: { size: number; radius?: number }) => (
  <span className="stop-square" style={{ width: size, height: size, borderRadius: radius }} />
);
