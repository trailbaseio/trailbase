import globals from "globals";
import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";
import tailwind from "eslint-plugin-better-tailwindcss";
import solid from "eslint-plugin-solid/configs/recommended";
import astro from "eslint-plugin-astro";

export default [
  {
    ignores: [
      "dist/",
      "node_modules/",
      ".astro/",
      "src/env.d.ts",
      "src/components/ui",
    ],
  },
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  solid,
  ...astro.configs.recommended,
  {
    plugins: {
      "better-tailwindcss": tailwind,
    },
    rules: {
      ...tailwind.configs["recommended-warn"].rules,
      ...tailwind.configs["recommended-error"].rules,

      "better-tailwindcss/enforce-consistent-line-wrapping": "off",
      // Order is different from what prettier enforces.
      "better-tailwindcss/enforce-consistent-class-order": "off",
      "better-tailwindcss/no-unknown-classes": [
        "error",
        {
          ignore: ["hide-scrollbars", "collapsible.*"],
        },
      ],
      // TODO: recently introduced, should look into solutions, e.g. ignore components/ui.
      "better-tailwindcss/enforce-canonical-classes": "warn",
      "better-tailwindcss/enforce-consistent-variable-syntax": "warn",
      "better-tailwindcss/enforce-shorthand-classes": "warn",
    },
    settings: {
      "better-tailwindcss": {
        entryPoint: "src/index.css",
      },
    },
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
  },
];
