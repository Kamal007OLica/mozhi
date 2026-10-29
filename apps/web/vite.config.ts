import { defineConfig, type Plugin } from "vite";
import { SITE } from "./src/site.config";

// Replaces %SITE.key% in index.html so links stay in one constant (src/site.config.ts).
function siteLinks(): Plugin {
  return {
    name: "mozhi-site-links",
    transformIndexHtml(html) {
      return html.replace(/%SITE\.(\w+)%/g, (_, key: keyof typeof SITE) => {
        if (!(key in SITE)) throw new Error(`Unknown %SITE.${key}% in index.html`);
        return SITE[key];
      });
    },
  };
}

export default defineConfig({
  plugins: [siteLinks()],
  server: { port: 4321 },
  build: { target: "es2020", cssMinify: true, assetsInlineLimit: 0 },
});
