import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevBackendIfNeeded } from "../shared/api";
import SettingsApp from "./SettingsApp";
import "../widget.css";
import "../shared/tokens.css";
import "./settings.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
document.body.classList.add("sp-app");

installDevBackendIfNeeded().then(() => {
  createRoot(root).render(
    <StrictMode>
      <SettingsApp />
    </StrictMode>,
  );
});
