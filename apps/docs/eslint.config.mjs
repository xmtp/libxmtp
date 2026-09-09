import js from "@eslint/js";
import astro from "eslint-plugin-astro";
import globals from "globals";
import tseslint from "typescript-eslint";

export default [
  {
    ignores: [
      "node_modules/**",
      "dist/**",
      ".astro/**",
      "_site/**",
      "._site-*/**",
      "playwright-report/**",
      "test-results/**",
      "generated/**",
    ],
  },
  js.configs.recommended,
  ...astro.configs.recommended,
  ...tseslint.configs.recommended.map((config) => ({
    ...config,
    files: ["examples/**/*.ts"],
  })),
  {
    files: ["examples/**/*.ts"],
    rules: {
      // Examples can name results for a reader to use in the next step.
      "@typescript-eslint/no-unused-vars": "off",
    },
  },
  { languageOptions: { globals: { ...globals.node, ...globals.browser } } },
];
