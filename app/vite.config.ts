import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite config for the Tauri frontend. Port must match tauri.conf.json devUrl.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "es2021",
    outDir: "dist",
  },
});
