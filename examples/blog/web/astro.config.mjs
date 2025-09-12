import { defineConfig } from "astro/config";

import icon from "astro-icon";
import solid from "@astrojs/solid-js";
import tailwindcss from "@tailwindcss/vite";

// https://astro.build/config
export default defineConfig({
  integrations: [icon(), solid()],
  vite: {
    plugins: [
      // @ts-expect-error: https://github.com/withastro/astro/issues/14030
      tailwindcss(),
    ],
  },
});
