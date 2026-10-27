import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The dev mock reads the Rust layout fixture from outside the app folder.
    fs: { allow: [".", "../core/tests/fixtures"] },
  },
});
