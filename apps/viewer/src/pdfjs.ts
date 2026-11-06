// O carregamento do pdf.js. O `pdf_viewer.mjs` (visualizador, busca, links) não importa a
// biblioteca: lê `globalThis.pdfjsLib`. Por isso este módulo publica a global **antes** de
// importá-lo, num `import()` dinâmico — um `import` estático seria içado para antes da
// atribuição e acharia a global vazia.

import * as pdfjsLib from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

(globalThis as typeof globalThis & { pdfjsLib: typeof pdfjsLib }).pdfjsLib = pdfjsLib;
pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

export const viewerLib = await import("pdfjs-dist/web/pdf_viewer.mjs");

export { pdfjsLib };

/**
 * Onde o pdf.js acha os recursos que busca em runtime — copiados para `public/pdfjs/` pelo
 * `vite.config.ts` e servidos pela origem do app.
 */
export const ASSET_OPTIONS = {
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  wasmUrl: "/pdfjs/wasm/",
  iccUrl: "/pdfjs/iccs/",
} as const;

export type PDFDocumentProxy = import("pdfjs-dist").PDFDocumentProxy;
export type PDFDocumentLoadingTask = import("pdfjs-dist").PDFDocumentLoadingTask;
export type PDFViewer = InstanceType<typeof viewerLib.PDFViewer>;
export type EventBus = InstanceType<typeof viewerLib.EventBus>;
export type PDFLinkService = InstanceType<typeof viewerLib.PDFLinkService>;
export type PDFFindController = InstanceType<typeof viewerLib.PDFFindController>;
