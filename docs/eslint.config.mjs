import globals from "globals";

import jsPlugin from "@eslint/js";
import tsPlugin from "typescript-eslint";
import tailwindPlugin from "eslint-plugin-better-tailwindcss";
import solidPlugin from "eslint-plugin-solid/configs/recommended";
import astroPlugin from "eslint-plugin-astro";

// Starlight utility classes used by forked layouts. Ideally, we'd install them
// from our `src/styles/global.css` so the linter just works, however we haven't
// found a good way to do so.
const ignoredStarlightCustomTailwindClasses = [
  "sl-.+",
  "hero",
  "hero-html",
  "stack",
  "copy",
  "tagline",
  "actions",
  "site-title",
  "site-title-link",
];

export default [
  {
    ignores: ["dist/", "node_modules/", ".astro/", "src/env.d.ts"],
  },
  jsPlugin.configs.recommended,
  ...tsPlugin.configs.recommended,
  // tailwindPlugin.configs["flat/recommended"],
  ...astroPlugin.configs.recommended,
  {
    plugins: {
      "better-tailwindcss": tailwindPlugin,
    },
    rules: {
      ...tailwindPlugin.configs["recommended-warn"].rules,
      ...tailwindPlugin.configs["recommended-error"].rules,

      "better-tailwindcss/enforce-consistent-line-wrapping": "off",
      // Order is different from what prettier enforces.
      "better-tailwindcss/enforce-consistent-class-order": "off",
      "better-tailwindcss/no-unregistered-classes": [
        "error",
        {
          ignore: ignoredStarlightCustomTailwindClasses,
        },
      ],
    },
    settings: {
      "better-tailwindcss": {
        entryPoint: "src/styles/global.css",
      },
    },
  },
  {
    files: ["**/*.{js,jsx,ts,tsx}"],
    ...solidPlugin,
  },
  {
    files: ["**/*.{js,mjs,cjs,mts,ts,tsx,jsx,astro}"],
    rules: {
      // https://typescript-eslint.io/rules/no-explicit-any/
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-wrapper-object-types": "warn",
      // http://eslint.org/docs/rules/no-unused-vars
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          vars: "all",
          args: "after-used",
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
        },
      ],
      // Collides with astro, we'd have to configure the solid plugin to ignore astro files.
      "solid/no-unknown-namespaces": "off",
      // Needed for astro.config.ts @ts-ignore in because @ts-expect-error doesn't work reliably across envs.
      "@typescript-eslint/ban-ts-comment": "warn",
    },
    languageOptions: { globals: globals.browser },
  },
];
