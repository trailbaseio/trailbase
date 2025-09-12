import { defineConfig } from "vite";

import tsconfigPaths from "vite-tsconfig-paths";
import solidPlugin from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  base: "/_/admin",
  plugins: [solidPlugin(), tsconfigPaths(), tailwindcss()],
  server: {
    port: 3000,
  },
  build: {
    target: "esnext",
  },
});
