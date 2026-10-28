import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default defineConfig([
  globalIgnores(["dist/**", "src-tauri/**", "src/bindings/**"]),
  {
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.strictTypeChecked,
      reactHooks.configs.flat.recommended,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // ARCH-00 §3.4: only src/core-api talks to Tauri.
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@tauri-apps/api(/.*)?$",
              message: "Only src/core-api may import @tauri-apps/api (ARCH-00 §3.4).",
            },
          ],
        },
      ],
      // THREAT-MODEL TM-16: never render raw HTML.
      "no-restricted-syntax": [
        "error",
        {
          selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']",
          message: "Rendering raw HTML is forbidden (THREAT-MODEL TM-16).",
        },
      ],
    },
  },
  {
    files: ["src/core-api/**/*.ts"],
    rules: { "no-restricted-imports": "off" },
  },
  {
    // Tests may use the official IPC mocks.
    files: ["src/**/*.test.{ts,tsx}", "src/test/**/*.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              regex: "^@tauri-apps/api(?!/mocks$)(/.*)?$",
              message: "Tests may import only @tauri-apps/api/mocks.",
            },
          ],
        },
      ],
    },
  },
]);
