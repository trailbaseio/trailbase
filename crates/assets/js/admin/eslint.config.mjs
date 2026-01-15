import globals from "globals";
import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";
import tailwind from "eslint-plugin-better-tailwindcss";
import solid from "eslint-plugin-solid/configs/recommended";

export default [
  {
    ignores: ["dist/", "node_modules/", "vite.config.mts"],
  },
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  solid,
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
          ignore: [
            // Kobalte?
            "duration-250ms",
            "items-top",
            "collapsible",
            "collapsible__trigger",
            "collapsible__content",
            // Ours:
            "hide-scrollbars",
          ],
        },
      ],
      // TODO: recently introduced. Should look for solutions, e.g. ignore components/ui.
      "better-tailwindcss/enforce-canonical-classes": "warn",
      "better-tailwindcss/enforce-consistent-variable-syntax": "warn",
      "better-tailwindcss/enforce-shorthand-classes": "warn",
      "better-tailwindcss/no-deprecated-classes": "warn",
      "better-tailwindcss/enforce-consistent-important-position": "warn",
    },
    settings: {
      "better-tailwindcss": {
        entryPoint: "src/index.css",
      },
    },
  },
  {
    files: ["**/*.{js,mjs,cjs,mts,ts,tsx,jsx}"],
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
    },
    languageOptions: { globals: globals.browser },
  },
];
