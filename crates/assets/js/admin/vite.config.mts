import { defineConfig } from "vite";

import tsconfigPaths from "vite-tsconfig-paths";
import solidPlugin from "vite-plugin-solid";
import csp from "vite-plugin-csp-guard";
import tailwindcss from "@tailwindcss/vite";

const DEFAULT_CSP = [
  "'self'",
  "chrome-extension:",
  "moz-extension:",
  "safari-extension:",
];

export default defineConfig({
  base: "/_/admin",
  plugins: [
    solidPlugin(),
    tsconfigPaths(),
    tailwindcss(),
    csp({
      dev: {
        // No CSP in dev mode.
        run: false,
      },
      policy: {
        "default-src": DEFAULT_CSP,

        // FIXME: We need the "*" for WASM dashboards in a non-allow-same-origin
        // sandboxed iframe because Firefox/Safari do not respect the iframe's
        // CSP:
        //   https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/csp
        "connect-src": ["'self'", "https://tiles.openfreemap.org", "*"],
        // "connect-src": ["'self'", "https://tiles.openfreemap.org"],
        "img-src": [...DEFAULT_CSP, "data:"],
        "object-src": DEFAULT_CSP,
        "script-src": [...DEFAULT_CSP, "blob:"],
        // WARN: We should definitely disallow eval() to avoid any potential
        // injections from DB contents via the admin table browser/explorer.
        "style-src": [...DEFAULT_CSP, "'unsafe-inline'"],
        // 'unsafe-inline' needed for ERD renderer.
        "style-src-elem": [...DEFAULT_CSP, "'unsafe-inline'"],
        "frame-src": ["'self'", "blob:"],
        "child-src": ["'self'", "blob:"],
      },
      build: {
        sri: true,
      },
    }),
  ],
  optimizeDeps: {
    include: ["maplibre-gl"],
    esbuildOptions: {
      target: "es2022",
    },
  },
  server: {
    port: 3000,
  },
  build: {
    target: "esnext",
  },
});
