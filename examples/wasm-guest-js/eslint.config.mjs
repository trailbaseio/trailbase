import pluginJs from "@eslint/js";
import globals from "globals";

export default [
  pluginJs.configs.recommended,
  {
    ignores: ["dist/", "node_modules/"],
  },
  {
    files: ["src/**/*.{js,mjs,cjs,mts,jsx}"],
    rules: {
      "no-var": "off",
    },
    languageOptions: {
      globals: {
        ...globals.browser,
      }
    }
  },
];
