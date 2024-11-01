import { defineConfig } from "astro/config";

import icon from "astro-icon";
import robotsTxt from "astro-robots-txt";
import sitemap from "@astrojs/sitemap";
import solid from "@astrojs/solid-js";
import starlight from "@astrojs/starlight";
import tailwind from "@astrojs/tailwind";

// https://astro.build/config
export default defineConfig({
  site: "https://trailbase.io",
  integrations: [
    icon(),
    solid(),
    starlight({
      title: "TrailBase",
      customCss: ["./src/tailwind.css"],
      social: {
        github: "https://github.com/trailbaseio/trailbase",
        discord: "https://discord.gg/X8cWs7YC22",
      },
      sidebar: [
        {
          label: "Getting Started",
          items: [
            {
              label: "Starting Up",
              slug: "getting-started/starting-up",
            },
            {
              label: "First App",
              slug: "getting-started/first-app",
            },
            {
              label: "Philosophy",
              slug: "getting-started/philosophy",
            },
          ],
        },
        {
          label: "Documentation",
          autogenerate: {
            directory: "documentation",
          },
        },
        {
          label: "Comparisons",
          autogenerate: {
            directory: "comparison",
          },
        },
        {
          label: "Reference",
          autogenerate: {
            directory: "reference",
          },
        },
      ],
    }),
    sitemap(),
    robotsTxt(),
    tailwind({
      // Disable the default base styles:
      applyBaseStyles: false,
    }),
  ],
});
