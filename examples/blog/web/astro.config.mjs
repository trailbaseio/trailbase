import { defineConfig } from "astro/config";

import icon from "astro-icon";
import solid from "@astrojs/solid-js";

// https://astro.build/config
export default defineConfig({
  integrations: [icon(), solid()],
  vite: {
    plugins: [
      // Need to use PostCSS for now: https://github.com/withastro/astro/issues/16542
      // tailwindcss(),
    ],
  },
});
