import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevBackendIfNeeded } from "../shared/api";
import HistoryApp from "./HistoryApp";
import "../widget.css";
import "../shared/tokens.css";
import "./history.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
document.body.classList.add("sp-app", "hi-page");

installDevBackendIfNeeded().then(() => {
  createRoot(root).render(
    <StrictMode>
      <HistoryApp />
    </StrictMode>,
  );
});
