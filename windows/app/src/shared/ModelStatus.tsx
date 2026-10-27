import type { ModelInfo } from "./api";

// The recognizer's download and load state, as a dot and a short phrase.
export default function ModelStatus({ model }: { model: ModelInfo }) {
  if (model.state === "downloading") {
    const percent = model.percent ?? 0;
    return (
      <span className="sp-status" data-model="downloading">
        <i className="sp-dot warn" />
        Downloading {percent}%
        <span className="sp-progress" aria-hidden="true">
          <i style={{ width: `${percent}%` }} />
        </span>
      </span>
    );
  }
  if (model.state === "loading") {
    return (
      <span className="sp-status" data-model="loading">
        <i className="sp-dot warn" />
        Loading
      </span>
    );
  }
  if (model.state === "failed") {
    return (
      <span className="sp-status" data-model="failed">
        <i className="sp-dot warn" />
        Could not load. Shepit tries again on the next dictation.
      </span>
    );
  }
  return (
    <span className="sp-status" data-model="ready">
      <i className="sp-dot ok" />
      Ready
    </span>
  );
}
