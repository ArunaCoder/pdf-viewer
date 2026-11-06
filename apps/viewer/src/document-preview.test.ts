import { describe, expect, it } from "vitest";

import { documentNotices, renderDocument } from "./document-preview";
import type { DocumentPreview } from "./ipc";

const NO_NOTES = { hasMacros: false, skippedImages: 0, skippedObjects: 0, truncated: false };

function preview(blocks: DocumentPreview["blocks"], images: DocumentPreview["images"] = []) {
  return renderDocument({ format: "docx", blocks, images, notes: NO_NOTES });
}

describe("renderDocument", () => {
  it("põe o texto do documento como texto, nunca como HTML", () => {
    const root = preview([
      {
        type: "paragraph",
        role: { kind: "body" },
        marker: null,
        align: "start",
        inlines: [{ type: "text", text: '<img src=x onerror="alert(1)">', marks: ["bold"] }],
      },
    ]);
    expect(root.querySelector("img")).toBeNull();
    expect(root.querySelector(".m-bold")?.textContent).toBe('<img src=x onerror="alert(1)">');
  });

  it("desenha títulos, itens de lista com marcador e alinhamento", () => {
    const root = preview([
      {
        type: "paragraph",
        role: { kind: "heading", level: 9 },
        marker: null,
        align: "center",
        inlines: [{ type: "text", text: "Título", marks: [] }],
      },
      {
        type: "paragraph",
        role: { kind: "list_item", level: 2 },
        marker: "1.a)",
        align: "start",
        inlines: [{ type: "text", text: "item", marks: [] }, { type: "break" }],
      },
    ]);
    const heading = root.querySelector("h6");
    expect(heading?.classList.contains("align-center")).toBe(true);
    const item = root.querySelector<HTMLElement>(".doc-li");
    expect(item?.querySelector(".doc-marker")?.textContent).toBe("1.a)");
    expect(item?.style.getPropertyValue("--level")).toBe("2");
    expect(item?.querySelector("br")).not.toBeNull();
  });

  it("monta a tabela com as mesclas e as imagens pela posição", () => {
    const root = preview(
      [
        {
          type: "table",
          rows: [
            {
              cells: [
                {
                  colSpan: 2,
                  rowSpan: 3,
                  blocks: [
                    {
                      type: "paragraph",
                      role: { kind: "body" },
                      marker: null,
                      align: "start",
                      inlines: [
                        { type: "image", index: 0 },
                        { type: "image", index: 7 },
                      ],
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
      [{ mimeType: "image/png", data: "iVBORw0KGgo=" }],
    );
    const cell = root.querySelector("td");
    expect(cell?.colSpan).toBe(2);
    expect(cell?.rowSpan).toBe(3);
    const images = root.querySelectorAll("img");
    expect(images).toHaveLength(1);
    expect(images[0]?.getAttribute("src")).toBe("data:image/png;base64,iVBORw0KGgo=");
  });
});

describe("documentNotices", () => {
  it("avisa a macro antes do que ficou de fora", () => {
    const notices = documentNotices({
      hasMacros: true,
      skippedImages: 2,
      skippedObjects: 1,
      truncated: true,
    });
    expect(notices.map((notice) => notice.tone)).toEqual(["warning", "info", "info"]);
    expect(notices[1]?.text).toBe(
      "Não exibidos na pré-visualização: 2 imagens e 1 objeto (gráfico, forma ou caixa de texto).",
    );
  });

  it("não diz nada do documento completo e sem macro", () => {
    expect(documentNotices(NO_NOTES)).toEqual([]);
  });
});
