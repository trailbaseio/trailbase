import globals from "globals";
import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";
import tailwind from "eslint-plugin-tailwindcss";
import solid from "eslint-plugin-solid/configs/recommended";
import astro from "eslint-plugin-astro";

export default [
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  solid,
  ...tailwind.configs["flat/recommended"],
  ...astro.configs.recommended,
  {
    ignores: ["dist/", "node_modules/", ".astro/", "src/env.d.ts"],
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
      // Prettier prefers explicit closing.
      "solid/self-closing-comp": "off",
    },
    languageOptions: { globals: globals.browser },
    settings: {
      tailwindcss: {
        whitelist: ["hide-scrollbars", "collapsible.*"],
      },
    },
  },
];
