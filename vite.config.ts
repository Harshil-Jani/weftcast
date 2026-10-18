import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import brand from "./brand.json";

export default defineConfig({
  plugins: [
    react(),
    { name: "brand-html", transformIndexHtml: (html) => html.replaceAll("%BRAND_NAME%", brand.name) },
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022" },
});
