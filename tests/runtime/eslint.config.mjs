import pluginJs from "@eslint/js";
import tseslint from "typescript-eslint";

export default [
  pluginJs.configs.recommended,
  ...tseslint.configs.recommended,
  {
    ignores: ["dist/", "node_modules/"],
  },
  {
    files: ["src/**/*.{js,mjs,cjs,mts,ts,tsx,jsx}"],
    rules: {
      // https://typescript-eslint.io/rules/no-explicit-any/
      "@typescript-eslint/no-explicit-any": "warn",
      "@typescript-eslint/no-wrapper-object-types": "warn",
      "@typescript-eslint/no-namespace": "off",
      "no-var": "off",
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
  },
];
