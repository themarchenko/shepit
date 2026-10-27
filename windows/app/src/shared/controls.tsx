import type { ReactNode } from "react";

// The grey-window controls. They hold no state of their own; the panes own the values.

export function KeyCap({ label, picking = false }: { label: string; picking?: boolean }) {
  return <span className={picking ? "sp-keycap picking" : "sp-keycap"}>{label}</span>;
}

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section>
      {title && <h2 className="sp-group-title">{title}</h2>}
      <div className="sp-group">{children}</div>
    </section>
  );
}

export function Row({ label, detail, children }: { label: string; detail?: ReactNode; children?: ReactNode }) {
  return (
    <div className="sp-row">
      <div className="sp-row-text">
        <span>{label}</span>
        {detail && <span className="sp-row-detail">{detail}</span>}
      </div>
      {children && <div className="sp-row-control">{children}</div>}
    </div>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="sp-toggle"
      onClick={() => onChange(!checked)}
    />
  );
}

export interface Option<T extends string> {
  value: T;
  label: string;
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: Option<T>[];
  value: T;
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <div className="sp-segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          data-value={o.value}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

// `onChange` fires while the thumb moves (for a live preview); `onCommit` fires when it is let go
// (to save). A keyboard step counts as let go on key up.
export function Slider({
  value,
  min,
  max,
  step,
  onChange,
  onCommit,
  label,
  minLabel,
  maxLabel,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  onCommit: (v: number) => void;
  label: string;
  minLabel: string;
  maxLabel: string;
}) {
  const fill = `${((value - min) / (max - min)) * 100}%`;
  const commit = () => onCommit(value);
  return (
    <div className="sp-slider">
      <span>{minLabel}</span>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        style={{ "--fill": fill } as React.CSSProperties}
        onChange={(e) => onChange(Number(e.currentTarget.value))}
        onPointerUp={commit}
        onKeyUp={commit}
      />
      <span>{maxLabel}</span>
    </div>
  );
}

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement>;

export function Button({ className = "", ...rest }: ButtonProps) {
  return <button type="button" className={`sp-btn ${className}`} {...rest} />;
}

export function PrimaryButton({ className = "", ...rest }: ButtonProps) {
  return <Button {...rest} className={`primary ${className}`} />;
}

export function QuietButton({ className = "", ...rest }: ButtonProps) {
  return <Button {...rest} className={`quiet ${className}`} />;
}

// A number with minus and plus buttons. Rust clamps the value too; the limits here keep the buttons honest.
export function Stepper({
  value,
  min,
  max,
  step = 1,
  onChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  label: string;
}) {
  return (
    <div className="sp-stepper" role="group" aria-label={label}>
      <button type="button" aria-label={`Less: ${label}`} disabled={value <= min} onClick={() => onChange(Math.max(min, value - step))}>
        -
      </button>
      <span aria-live="polite">{value}</span>
      <button type="button" aria-label={`More: ${label}`} disabled={value >= max} onClick={() => onChange(Math.min(max, value + step))}>
        +
      </button>
    </div>
  );
}
