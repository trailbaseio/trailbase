import { defineConfig } from "astro/config";

import solidJs from "@astrojs/solid-js";
import icon from "astro-icon";

// https://astro.build/config
export default defineConfig({
  output: "static",
  base: "/_/auth",
  integrations: [icon(), solidJs()],
  vite: {
    plugins: [
      // Need to use PostCSS for now: https://github.com/withastro/astro/issues/16542
      // tailwindcss(),
    ],
  },
});
