import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Widget, { type MockConfig } from "./Widget";
import "./widget.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
const container = root;

function mount(preset?: MockConfig) {
  createRoot(container).render(
    <StrictMode>
      <Widget preset={preset} />
    </StrictMode>,
  );
}

if (import.meta.env.DEV) {
  // Dev only: the mock and its fixture are not part of the production bundle.
  import("./mock").then(({ mockFromLocation }) => mount(mockFromLocation()));
} else {
  mount();
}
