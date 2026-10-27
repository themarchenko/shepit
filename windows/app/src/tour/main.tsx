import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevBackendIfNeeded } from "../shared/api";
import TourApp from "./TourApp";
import "../widget.css";
import "../shared/tokens.css";
import "./tour.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
document.body.classList.add("sp-app", "sp-tour-page");

installDevBackendIfNeeded().then(() => {
  createRoot(root).render(
    <StrictMode>
      <TourApp />
    </StrictMode>,
  );
});
