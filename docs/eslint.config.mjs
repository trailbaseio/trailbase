import globals from "globals";

import jsPlugin from "@eslint/js";
import tsPlugin from "typescript-eslint";
import tailwindPlugin from "eslint-plugin-tailwindcss";
import solidPlugin from "eslint-plugin-solid/configs/recommended";
import astroPlugin from "eslint-plugin-astro";

console.info(
  "TODO: Tailwind v4 eslint missing (https://github.com/francoismassart/eslint-plugin-tailwindcss/issues/325): ",
  Object.keys(tailwindPlugin),
);

export default [
  {
    ignores: ["dist/", "node_modules/", ".astro/", "src/env.d.ts"],
  },
  jsPlugin.configs.recommended,
  ...tsPlugin.configs.recommended,
  // tailwindPlugin.configs["flat/recommended"],
  ...astroPlugin.configs.recommended,
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
    },
    languageOptions: { globals: globals.browser },
    settings: {
      tailwindcss: {
        whitelist: ["hide-scrollbars", "collapsible.*"],
      },
    },
  },
];
