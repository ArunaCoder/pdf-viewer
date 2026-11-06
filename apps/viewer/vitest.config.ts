import { defineConfig } from "vitest/config";

// A suíte cobre a lógica pura do webview (formatação, tradução de erro, ordem de abas):
// o que depende do pdf.js ou do IPC se valida no app, e a régua dos anexos mora no Rust.
export default defineConfig({
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
  },
});
