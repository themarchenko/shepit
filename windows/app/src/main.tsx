import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import Widget from "./Widget";
import "./widget.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

createRoot(root).render(
  <StrictMode>
    <Widget />
  </StrictMode>,
);
