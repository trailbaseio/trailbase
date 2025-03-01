import globals from "globals";
import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";
import tailwind from "eslint-plugin-tailwindcss";
import solid from "eslint-plugin-solid/configs/recommended";

export default [
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  solid,
  ...tailwind.configs["flat/recommended"],
  {
    ignores: ["dist/", "node_modules/", "vite.config.mts"],
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
      // FIXME
      "solid/reactivity": "off",
    },
    languageOptions: { globals: globals.browser },
    settings: {
      tailwindcss: {
        whitelist: ["hide-scrollbars", "collapsible.*"],
      },
    },
  },
];
