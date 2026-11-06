// A prévia de leitura de um documento .docx ou .odt anexado.
//
// O Rust já leu o arquivo e devolve blocos com tipo — parágrafo, tabela, trecho marcado,
// imagem com o formato conferido pela assinatura. Aqui só se monta o DOM pelo `el`: todo
// texto do documento entra como nó de texto, e nada dele é interpretado como HTML.

import { el } from "./dom";
import type {
  DocumentBlock,
  DocumentCell,
  DocumentInline,
  DocumentNotes,
  DocumentPreview,
} from "./ipc";

const HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"] as const;

/** O documento pronto para pôr no palco da prévia. */
export function renderDocument(preview: DocumentPreview): HTMLElement {
  const images = preview.images.map((image) => `data:${image.mimeType};base64,${image.data}`);
  const notices = documentNotices(preview.notes);
  return el("div", { class: "preview-doc" }, [
    notices.length > 0
      ? el(
          "ul",
          { class: "doc-notices" },
          notices.map((notice) =>
            el("li", { class: notice.tone === "warning" ? "warning" : "", text: notice.text }),
          ),
        )
      : null,
    el(
      "article",
      { class: "doc-page" },
      preview.blocks.map((block) => renderBlock(block, images)),
    ),
  ]);
}

export interface Notice {
  tone: "warning" | "info";
  text: string;
}

/** Os avisos da prévia: macro primeiro, depois o que ficou de fora. */
export function documentNotices(notes: DocumentNotes): Notice[] {
  const notices: Notice[] = [];
  if (notes.hasMacros) {
    notices.push({
      tone: "warning",
      text: "Este documento contém macros. Elas não rodam na pré-visualização; se abrir o arquivo no Word ou no LibreOffice, não habilite o conteúdo.",
    });
  }
  const left: string[] = [];
  if (notes.skippedImages > 0) {
    left.push(notes.skippedImages === 1 ? "1 imagem" : `${notes.skippedImages} imagens`);
  }
  if (notes.skippedObjects > 0) {
    left.push(
      notes.skippedObjects === 1
        ? "1 objeto (gráfico, forma ou caixa de texto)"
        : `${notes.skippedObjects} objetos (gráficos, formas ou caixas de texto)`,
    );
  }
  if (left.length > 0) {
    notices.push({ tone: "info", text: `Não exibidos na pré-visualização: ${left.join(" e ")}.` });
  }
  if (notes.truncated) {
    notices.push({
      tone: "info",
      text: "O documento é longo demais para a pré-visualização; só o começo é mostrado.",
    });
  }
  return notices;
}

function renderBlock(block: DocumentBlock, images: readonly string[]): HTMLElement {
  if (block.type === "table") {
    return el("table", { class: "doc-table" }, [
      el(
        "tbody",
        {},
        block.rows.map((row) =>
          el(
            "tr",
            {},
            row.cells.map((cell) => renderCell(cell, images)),
          ),
        ),
      ),
    ]);
  }
  const content = block.inlines.map((inline) => renderInline(inline, images));
  const marker =
    block.marker === null ? null : el("span", { class: "doc-marker", text: block.marker });
  const align = `align-${block.align}`;
  switch (block.role.kind) {
    case "heading": {
      const tag = HEADINGS[Math.min(Math.max(block.role.level, 1), 6) - 1] ?? "h6";
      return el(tag, { class: `doc-heading ${align}` }, [marker, ...content]);
    }
    case "list_item": {
      const item = el("p", { class: `doc-p doc-li ${align}` }, [marker, ...content]);
      item.style.setProperty("--level", String(block.role.level));
      return item;
    }
    case "body":
      return el("p", { class: `doc-p ${align}` }, content);
  }
}

function renderCell(cell: DocumentCell, images: readonly string[]): HTMLElement {
  const td = el(
    "td",
    {},
    cell.blocks.map((block) => renderBlock(block, images)),
  );
  if (cell.colSpan > 1) td.colSpan = cell.colSpan;
  if (cell.rowSpan > 1) td.rowSpan = cell.rowSpan;
  return td;
}

function renderInline(inline: DocumentInline, images: readonly string[]): Node | string | null {
  switch (inline.type) {
    case "text":
      return inline.marks.length === 0
        ? inline.text
        : el("span", {
            class: inline.marks.map((mark) => `m-${mark}`).join(" "),
            text: inline.text,
          });
    case "break":
      return el("br");
    case "image": {
      const src = images[inline.index];
      return src === undefined ? null : el("img", { class: "doc-image", attrs: { src, alt: "" } });
    }
  }
}
