import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // During `bun run dev`, forward API calls to a running `gatebase ui` proxy.
    proxy: {
      "/api": "http://127.0.0.1:7777",
    },
  },
});
