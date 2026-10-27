// WidgetPalette.spring is SwiftUI's spring(response: 0.4, dampingFraction: 0.72). That is a damped
// harmonic oscillator with natural frequency w0 = 2*pi/response and damping ratio zeta. Its step response
// (0 to 1, starting at rest) is
//   x(t) = 1 - e^(-zeta*w0*t) * (cos(wd*t) + zeta*w0/wd * sin(wd*t)),  wd = w0*sqrt(1 - zeta^2)
// CSS has no spring easing, so the curve is sampled into linear() at load. It overshoots about 3%, as
// the macOS widget does. By 600 ms the envelope has decayed to 0.1%, so the last point is exactly 1.
const RESPONSE = 0.4;
const DAMPING = 0.72;

export const SPRING_MS = 600;

export function springEasing(points = 40): string {
  const w0 = (2 * Math.PI) / RESPONSE;
  const wd = w0 * Math.sqrt(1 - DAMPING * DAMPING);
  const decay = DAMPING * w0;
  const samples: string[] = [];
  for (let i = 0; i <= points; i++) {
    const t = (i / points) * (SPRING_MS / 1000);
    const x = 1 - Math.exp(-decay * t) * (Math.cos(wd * t) + (decay / wd) * Math.sin(wd * t));
    samples.push((i === points ? 1 : x).toFixed(4));
  }
  return `linear(${samples.join(", ")})`;
}

export const SPRING = springEasing();
