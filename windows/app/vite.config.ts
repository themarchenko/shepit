import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  build: {
    // The widget, Settings, the tour and the update card are separate windows with their own page.
    rollupOptions: {
      input: {
        main: "index.html",
        settings: "settings.html",
        tour: "tour.html",
        update: "update.html",
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    // The dev mock reads the Rust layout fixture and the update card fixture from outside the app folder.
    fs: { allow: [".", "../core/tests/fixtures", "../src-tauri/tests/fixtures"] },
  },
});
