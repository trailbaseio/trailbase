import { defineConfig } from "vite";

import tsconfigPaths from "vite-tsconfig-paths";
import solidPlugin from "vite-plugin-solid";
import csp from "vite-plugin-csp-guard";
import tailwindcss from "@tailwindcss/vite";

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
        "default-src": ["'self'"],

        "connect-src": ["'self'", "https://tiles.openfreemap.org"],
        "img-src": ["'self'", "data:"],
        "script-src": ["'self'", "blob:"],
        "style-src": ["'self'", "'unsafe-inline'"],
        // 'unsafe-inline' needed for ERD renderer.
        "style-src-elem": ["'self'", "'unsafe-inline'"],
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
