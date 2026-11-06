import { describe, expect, it } from "vitest";

import { decodeText } from "./attachments";
import { errorMessage, IpcFailure, toFailure } from "./ipc";
import { comboOf } from "./shortcuts";
import { cycleIndex, indexAfterClose, insertionIndex } from "./tab-order";

describe("ordem das abas", () => {
  it("ativa a da direita ao fechar, ou a da esquerda quando era a última", () => {
    expect(indexAfterClose(1, 4)).toBe(1);
    expect(indexAfterClose(3, 4)).toBe(2);
    expect(indexAfterClose(0, 1)).toBeNull();
  });

  it("dá a volta nas pontas com Ctrl+Tab", () => {
    expect(cycleIndex(2, 3, 1)).toBe(0);
    expect(cycleIndex(0, 3, -1)).toBe(2);
    expect(cycleIndex(0, 0, 1)).toBe(0);
  });

  it("põe o anexo aberto ao lado do documento de onde ele saiu", () => {
    expect(insertionIndex(1, 4)).toBe(2);
    expect(insertionIndex(null, 4)).toBe(4);
  });
});

describe("comboOf", () => {
  const key = (
    k: string,
    mods: Partial<Record<"ctrlKey" | "metaKey" | "shiftKey" | "altKey", boolean>> = {},
  ) => ({
    key: k,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...mods,
  });

  it("normaliza modificadores em ordem fixa e a tecla em minúsculas", () => {
    expect(comboOf(key("A", { ctrlKey: true, shiftKey: true }))).toBe("ctrl+shift+a");
    expect(comboOf(key("F11"))).toBe("f11");
    expect(comboOf(key(" "))).toBe("space");
    expect(comboOf(key("o", { metaKey: true }))).toBe("ctrl+o");
  });
});

describe("erros do IPC", () => {
  it("reconhece o erro na forma do contrato", () => {
    const failure = toFailure({
      code: "wrong_password",
      message: "the password does not open the PDF",
    });
    expect(failure).toBeInstanceOf(IpcFailure);
    expect(failure.code).toBe("wrong_password");
    expect(failure.message).toBe(errorMessage("wrong_password"));
    expect(failure.detail).toBe("the password does not open the PDF");
  });

  it("trata o que não vem na forma do contrato como interno", () => {
    expect(toFailure("boom").code).toBe("internal");
    expect(toFailure({ code: 3 }).code).toBe("internal");
  });
});

describe("decodeText", () => {
  const bytes = (values: number[]): ArrayBuffer => new Uint8Array(values).buffer;

  it("lê UTF-8 e cai para Windows-1252 quando não é UTF-8", () => {
    expect(decodeText(new TextEncoder().encode("ação").buffer as ArrayBuffer)).toBe("ação");
    // "ação" em Windows-1252: ç = 0xE7, ã = 0xE3.
    expect(decodeText(bytes([0x61, 0xe7, 0xe3, 0x6f]))).toBe("ação");
  });

  it("respeita o BOM de UTF-16", () => {
    expect(decodeText(bytes([0xff, 0xfe, 0x61, 0x00]))).toBe("a");
    expect(decodeText(bytes([0xfe, 0xff, 0x00, 0x61]))).toBe("a");
  });
});
