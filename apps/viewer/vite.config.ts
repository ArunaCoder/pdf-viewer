import { cpSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { defineConfig } from "vite";

// A porta é fixa (strictPort) porque o tauri.conf.json aponta para ela no devUrl.
const host = process.env.TAURI_DEV_HOST;

/// Os recursos que o pdf.js busca em runtime, por URL, e que o bundler não enxerga: os
/// CMaps das fontes CJK, as 14 fontes padrão, os decodificadores WebAssembly (JPEG 2000,
/// JBIG2, perfis de cor) e o perfil ICC. Copiados para `public/pdfjs/` a cada build — a
/// pasta é derivada e ignorada pelo git —, saem no `dist/` servidos pela própria origem do
/// app, que é o que a CSP `default-src 'self'` permite.
const PDFJS_ASSETS = ["cmaps", "standard_fonts", "wasm", "iccs"];

function copyPdfjsAssets(): void {
  for (const dir of PDFJS_ASSETS) {
    const from = fileURLToPath(new URL(`node_modules/pdfjs-dist/${dir}`, import.meta.url));
    const to = fileURLToPath(new URL(`public/pdfjs/${dir}`, import.meta.url));
    cpSync(from, to, { recursive: true });
  }
}

copyPdfjsAssets();

export default defineConfig({
  // Tauri cuida do clear screen; preserva os logs do Rust no terminal.
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    host: host ?? false,
    ...(host ? { hmr: { protocol: "ws", host, port: 1431 } } : {}),
    // A recompilação de src-tauri é do Cargo.
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebView2 é Chromium atual: o `await` de topo de módulo que carrega o pdf.js
    // (`src/pdfjs.ts`) não precisa de transpilação.
    target: "es2022",
    // O worker do pdf.js tem ~1,3 MB minificado, e o app o carrega do disco, não da rede:
    // o aviso de chunk grande do Vite é sobre tempo de download, que aqui não existe.
    chunkSizeWarningLimit: 1400,
  },
});
