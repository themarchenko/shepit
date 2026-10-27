import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDevBackendIfNeeded } from "../shared/api";
import UpdateCard from "./UpdateCard";
import "../widget.css";
import "../shared/tokens.css";
import "./update.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
document.body.classList.add("sp-app", "sp-update-page");

installDevBackendIfNeeded().then(() => {
  createRoot(root).render(
    <StrictMode>
      <UpdateCard />
    </StrictMode>,
  );
});
