import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const repoRoot = path.resolve(import.meta.dirname, "..");

export default defineConfig({
  output: "static",
  devToolbar: { enabled: false },
  integrations: [react()],
  vite: {
    plugins: [tailwindcss()],
    resolve: {
      alias: {
        "@": path.resolve(repoRoot, "src"),
      },
      dedupe: ["react", "react-dom"],
    },
    server: {
      fs: {
        allow: [repoRoot],
      },
    },
  },
});
