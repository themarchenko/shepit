import { showTour } from "../shared/api";
import { Button, Group, Row } from "../shared/controls";
import type { PaneProps } from "../shared/useSettings";

export default function AboutPane({ settings }: PaneProps) {
  return (
    <>
      <Group>
        <div className="sp-about">
          <div className="sp-appicon" aria-hidden="true">
            <svg width="34" height="34" viewBox="0 0 34 34" fill="#f4f2ee">
              {[6, 12, 20, 14, 8].map((h, i) => (
                <rect key={i} x={4 + i * 6} y={17 - h / 2} width="3.4" height={h} rx="1.7" />
              ))}
            </svg>
          </div>
          <div>
            <div className="sp-about-title">Shepit {settings.version}</div>
            <p className="sp-note">Speech recognition runs on this PC. Your audio never leaves it.</p>
          </div>
        </div>
      </Group>
      <Group>
        <Row label="Welcome tour">
          <Button onClick={() => showTour().catch(() => {})}>Show again</Button>
        </Row>
      </Group>
      <Group title="Credits">
        <div className="sp-block">
          <p className="sp-note">
            Speech model: NVIDIA Parakeet v3, repackaged for ONNX by istupakov. Licensed under CC-BY-4.0
            (creativecommons.org/licenses/by/4.0).
          </p>
        </div>
      </Group>
    </>
  );
}
