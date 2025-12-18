import { defineConfig } from "astro/config";

import starlightOpenAPI, { openAPISidebarGroups } from "starlight-openapi";
import icon from "astro-icon";
import robotsTxt from "astro-robots-txt";
import sitemap from "@astrojs/sitemap";
import solid from "@astrojs/solid-js";
import starlight from "@astrojs/starlight";
import starlightLinksValidator from "starlight-links-validator";
import tailwindcss from "@tailwindcss/vite";

import config from "./src/config";

const openApiBase = "api";

// https://astro.build/config
export default defineConfig({
  site: config.site,
  // NOTE: Since we're serving static content, these redirects are actual
  // pages with a meta refresh tag rather than redirect HTTP responses.
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
      title: config.title,
      description: config.description,
      editLink: {
        baseUrl: "https://github.com/trailbaseio/trailbase/edit/main/docs/",
      },
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
        // Generate the OpenAPI documentation pages.
        starlightOpenAPI([
          {
            base: openApiBase,
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
        starlightLinksValidator({
          exclude: [
            "http://localhost:4000/",
            "http://localhost:4000/**/*",
            // The link validator fails to validate the OpenAPI pages injected above.
            `/${openApiBase}/**/*`,
            "/blog/**/*",
          ],
        }),
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
                  label: "Realtime-Sync & SSR",
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
              label: "Endpoints",
              items: [
                {
                  label: "Overview",
                  slug: "documentation/apis_overview",
                },
                {
                  slug: "documentation/apis_record",
                },
                {
                  slug: "documentation/apis_js",
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
        Footer: "./src/components/layout/Footer.astro",
        Hero: "./src/components/layout/Hero.astro",
        SiteTitle: "./src/components/layout/SiteTitle.astro",
      },
    }),
  ],
  vite: {
    plugins: [
      // @ts-ignore
      tailwindcss(),
    ],
  },
});
