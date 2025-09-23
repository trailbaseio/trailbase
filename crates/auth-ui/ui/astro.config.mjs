import { defineConfig } from "astro/config";

import solidJs from "@astrojs/solid-js";
import icon from "astro-icon";
import tailwindcss from "@tailwindcss/vite";

// https://astro.build/config
export default defineConfig({
  output: "static",
  base: "/_/auth",
  integrations: [icon(), solidJs()],
  vite: {
    plugins: [
      // @ts-expect-error: https://github.com/withastro/astro/issues/14030
      tailwindcss(),
    ],
  },
});
