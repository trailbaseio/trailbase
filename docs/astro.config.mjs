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
              label: "OpenAPI",
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
                  slug: "getting-started/first-ui-app",
                },
                {
                  label: "Data CLI",
                  slug: "getting-started/first-cli-app",
                },
                {
                  label: "Realtime-sync & SSR",
                  slug: "getting-started/first-realtime-app",
                },
              ],
            },
            {
              label: "Our Goals",
              slug: "getting-started/goals",
            },
          ],
        },
        {
          label: "Documentation",
          items: [
            {
              slug: "documentation/auth",
            },
            {
              label: "APIs",
              items: [
                {
                  slug: "documentation/record_apis",
                },
                {
                  slug: "documentation/js_apis",
                },
              ],
            },
            {
              slug: "documentation/models_and_relations",
            },
            {
              slug: "documentation/migrations",
            },
            {
              slug: "documentation/type_safety",
            },
            {
              slug: "documentation/production",
            },
            {
              slug: "documentation/extending",
            },
          ],
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
        // Add the generated sidebar group to the sidebar.
        ...openAPISidebarGroups,
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
