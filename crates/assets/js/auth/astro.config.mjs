import { defineConfig } from "astro/config";

import solidJs from "@astrojs/solid-js";
import icon from "astro-icon";
import tailwind from "@astrojs/tailwind";

// https://astro.build/config
export default defineConfig({
  output: "static",
  base: "/_/auth",
  integrations: [
    icon(),
    solidJs(),
    tailwind({
      applyBaseStyles: false,
    }),
  ],
});
