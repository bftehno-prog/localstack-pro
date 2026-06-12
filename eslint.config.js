import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist", "node_modules", "src-tauri/target"]
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["scripts/*.mjs"],
    languageOptions: {
      globals: {
        console: "readonly",
        document: "readonly",
        fetch: "readonly",
        getComputedStyle: "readonly",
        location: "readonly",
        process: "readonly",
        setTimeout: "readonly",
        window: "readonly"
      }
    }
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: {
      "react-hooks": reactHooks
    },
    languageOptions: {
      parserOptions: {
        ecmaFeatures: { jsx: true }
      },
      globals: {
        crypto: "readonly",
        document: "readonly",
        FileList: "readonly",
        HTMLDivElement: "readonly",
        HTMLElement: "readonly",
        HTMLInputElement: "readonly",
        KeyboardEvent: "readonly",
        localStorage: "readonly",
        MutationObserver: "readonly",
        navigator: "readonly",
        window: "readonly"
      }
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": ["warn", { "argsIgnorePattern": "^_" }],
      "no-empty": ["error", { "allowEmptyCatch": true }]
    }
  }
);
