import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  server: {
    host: process.env.TAURI_DEV_HOST || false
  },
  preview: {
    port: 4173,
    strictPort: true
  }
});
