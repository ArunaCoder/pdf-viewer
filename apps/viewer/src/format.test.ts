import { describe, expect, it } from "vitest";

import {
  attachmentMeta,
  countLabel,
  formatBytes,
  formatIsoDate,
  formatZoom,
  originLabel,
  typeLabel,
} from "./format";
import type { Attachment } from "./ipc";

function attachment(overrides: Partial<Attachment>): Attachment {
  return {
    id: 0,
    name: "nota.xml",
    description: null,
    mimeType: null,
    declaredSize: null,
    created: null,
    modified: null,
    hasChecksum: false,
    relationship: null,
    origin: { kind: "document" },
    kind: "text",
    executable: false,
    ...overrides,
  };
}

describe("formatBytes", () => {
  it("usa a maior unidade abaixo de 1024, com vírgula decimal", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1,5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB");
    expect(formatBytes(3.25 * 1024 ** 3)).toBe("3,3 GB");
  });

  it("não inventa tamanho para valor inválido", () => {
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
});

describe("formatIsoDate", () => {
  it("formata a data do núcleo e recusa a que não é data", () => {
    expect(formatIsoDate("2024-03-09T14:05:00")).toBe("09/03/2024, 14:05");
    expect(formatIsoDate("não é data")).toBeNull();
    expect(formatIsoDate(null)).toBeNull();
  });
});

describe("rótulos de anexo", () => {
  it("conta no singular e no plural", () => {
    expect(countLabel(1, "anexo", "anexos")).toBe("1 anexo");
    expect(countLabel(0, "anexo", "anexos")).toBe("0 anexos");
  });

  it("diz onde o anexo está preso", () => {
    expect(originLabel({ kind: "document" })).toBe("Documento");
    expect(originLabel({ kind: "page", number: 4 })).toBe("Página 4");
  });

  it("usa a extensão curta como tipo, e o rótulo do tipo na falta dela", () => {
    expect(typeLabel(attachment({ name: "fatura.xml" }))).toBe("XML");
    expect(typeLabel(attachment({ name: "LEIA-ME", kind: "text" }))).toBe("Texto");
    expect(typeLabel(attachment({ name: "x.extensaolonga", kind: "other" }))).toBe("Arquivo");
    expect(typeLabel(attachment({ name: ".bashrc", kind: "other" }))).toBe("Arquivo");
  });

  it("monta a linha de metadados só com o que existe", () => {
    expect(
      attachmentMeta(attachment({ declaredSize: 2048, origin: { kind: "page", number: 2 } })),
    ).toBe("XML · 2 KB · Página 2");
    expect(attachmentMeta(attachment({}))).toBe("XML · Documento");
  });
});

describe("formatZoom", () => {
  it("arredonda para porcentagem inteira", () => {
    expect(formatZoom(1)).toBe("100%");
    expect(formatZoom(1.2549)).toBe("125%");
  });
});
