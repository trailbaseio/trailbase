import { defineConfig } from "astro/config";

import starlightOpenAPI, { openAPISidebarGroups } from "starlight-openapi";
import icon from "astro-icon";
import robotsTxt from "astro-robots-txt";
import sitemap from "@astrojs/sitemap";
import solid from "@astrojs/solid-js";
import starlight from "@astrojs/starlight";
import starlightLinksValidator from "starlight-links-validator";
import tailwindcss from "@tailwindcss/vite";

// https://astro.build/config
export default defineConfig({
  site: "https://trailbase.io",
  redirects: {
    // Stable docs path independent of documentation structure.
    "/docs": "/getting-started/install",
  },
  integrations: [
    icon(),
    robotsTxt(),
    sitemap(),
    solid(),
    starlight({
      title: "TrailBase",
      customCss: ["./src/styles/global.css"],
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/trailbaseio/trailbase",
        },
        {
          icon: "discord",
          label: "Discord",
          href: "https://discord.gg/X8cWs7YC22",
        },
      ],
      plugins: [
        starlightLinksValidator({
          exclude: ["http://localhost:4000/", "http://localhost:4000/**/*"],
        }),
        // Generate the OpenAPI documentation pages.
        starlightOpenAPI([
          {
            base: "api",
            schema: "./openapi/schema.json",
            sidebar: {
              label: "HTTP API",
              operations: {
                badges: true,
                labels: "operationId",
                sort: "alphabetical",
              },
            },
          },
        ]),
      ],
      sidebar: [
        {
          label: "Getting Started",
          items: [
            {
              label: "Install & Run",
              slug: "getting-started/install",
            },
            {
              label: "Tutorials",
              items: [
                {
                  label: "API, Vector Search & UI",
                  slug: "tutorials/first-ui-app",
                },
                {
                  label: "Data CLI",
                  slug: "tutorials/first-cli-app",
                },
                {
                  label: "Realtime-sync & SSR",
                  slug: "tutorials/first-realtime-app",
                },
              ],
            },
          ],
        },
        {
          label: "Guides",
          items: [
            {
              label: "Authentication",
              slug: "getting-started/auth",
            },
            {
              label: "Autgenerated Record APIs",
              slug: "getting-started/record_apis",
            },
            {
              label: "Extending with Custom APIs",
              slug: "getting-started/extending",
            },
            {
              slug: "getting-started/models_and_relations",
            },
            {
              slug: "getting-started/type_safety",
            },
            {
              label: "Going to production",
              slug: "getting-started/production",
            },
          ],
        },
        // Add the generated sidebar group to the sidebar.
        ...openAPISidebarGroups,
        {
          label: "Why TrailBase?",
          items: [
            {
              label: "Goals",
              slug: "why-trailbase/goals",
            },
            {
              label: "Comparisons",
              slug: "why-trailbase/comparisons",
            },
            {
              label: "Benchmarks",
              slug: "why-trailbase/benchmarks",
            },
            {
              label: "Roadmap",
              slug: "why-trailbase/roadmap",
            },
            {
              label: "FAQ",
              slug: "why-trailbase/faq",
            },
          ],
        },
      ],
      components: {
        Footer: "./src/components/Footer.astro",
      },
    }),
  ],
  vite: {
    plugins: [tailwindcss()],
  },
});
