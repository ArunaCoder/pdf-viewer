// Os diálogos do documento: a senha do PDF protegido e as propriedades.

import { el } from "./dom";
import { formatBytes, formatDate } from "./format";
import { type PDFDocumentProxy, pdfjsLib } from "./pdfjs";
import { button, modal } from "./ui";

/**
 * Pede a senha do PDF; `null` se o usuário desistir. `retry` diz que a anterior estava
 * errada, e o diálogo o mostra.
 */
export function askPassword(name: string, retry: boolean): Promise<string | null> {
  const { body, footer, close, closed } = modal({
    title: "PDF protegido",
    subtitle: name,
    icon: "lock",
    size: "sm",
  });
  const input = el("input", {
    class: "input",
    attrs: {
      type: "password",
      placeholder: "Senha do documento",
      autocomplete: "off",
      spellcheck: "false",
      "aria-label": "Senha do documento",
    },
  });
  let answer: string | null = null;
  const submit = (): void => {
    if (input.value.length === 0) {
      input.focus();
      return;
    }
    answer = input.value;
    close();
  };
  const form = el(
    "form",
    {
      class: "password-form",
      on: {
        submit: (event) => {
          event.preventDefault();
          submit();
        },
      },
    },
    [
      el("p", {
        class: "modal-text",
        text: "Este documento é cifrado. Digite a senha para abri-lo — ela também libera a leitura dos anexos.",
      }),
      input,
      retry
        ? el("p", {
            class: "field-error",
            text: "Senha incorreta. Tente de novo.",
          })
        : null,
    ],
  );
  body.append(form);
  footer.append(
    button({ label: "Cancelar", onClick: close }),
    button({ label: "Desbloquear", variant: "primary", onClick: submit }),
  );
  input.focus();
  return closed.then(() => answer);
}

interface InfoDictionary {
  Title?: unknown;
  Author?: unknown;
  Subject?: unknown;
  Keywords?: unknown;
  Creator?: unknown;
  Producer?: unknown;
  CreationDate?: unknown;
  ModDate?: unknown;
  PDFFormatVersion?: unknown;
  IsLinearized?: unknown;
  IsAcroFormPresent?: unknown;
  IsXFAPresent?: unknown;
  IsSignaturesPresent?: unknown;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

function pdfDate(value: unknown): string | null {
  return typeof value === "string" ? formatDate(pdfjsLib.PDFDateString.toDateObject(value)) : null;
}

function yesNo(value: unknown): string {
  return value === true ? "Sim" : "Não";
}

/** As propriedades do documento: metadados do PDF e do arquivo. */
export async function showProperties(
  pdf: PDFDocumentProxy,
  name: string,
  path: string | null,
  size: number,
): Promise<void> {
  const { body, footer, close } = modal({
    title: "Propriedades",
    subtitle: name,
    icon: "info",
    size: "md",
  });
  const loading = el("p", { class: "modal-text", text: "Lendo metadados…" });
  body.append(loading);
  footer.append(button({ label: "Fechar", variant: "primary", onClick: close }));

  const [{ info }, firstPage] = await Promise.all([pdf.getMetadata(), pdf.getPage(1)]);
  const dictionary = info as InfoDictionary;
  const viewport = firstPage.getViewport({ scale: 1 });
  // 72 pontos por polegada, 25,4 mm por polegada.
  const millimetres = (points: number): string => String(Math.round((points / 72) * 25.4));

  const rows: Array<[string, string | null]> = [
    ["Título", text(dictionary.Title)],
    ["Autor", text(dictionary.Author)],
    ["Assunto", text(dictionary.Subject)],
    ["Palavras-chave", text(dictionary.Keywords)],
    ["Criado em", pdfDate(dictionary.CreationDate)],
    ["Modificado em", pdfDate(dictionary.ModDate)],
    ["Aplicativo", text(dictionary.Creator)],
    ["Produtor do PDF", text(dictionary.Producer)],
    ["Versão do PDF", text(dictionary.PDFFormatVersion)],
    ["Páginas", String(pdf.numPages)],
    ["Tamanho da página", `${millimetres(viewport.width)} × ${millimetres(viewport.height)} mm`],
    ["Formulário", yesNo(dictionary.IsAcroFormPresent)],
    ["Assinaturas digitais", yesNo(dictionary.IsSignaturesPresent)],
    ["Otimizado para web", yesNo(dictionary.IsLinearized)],
    ["Tamanho do arquivo", formatBytes(size)],
    ["Local", path ?? "Aberto a partir de um anexo"],
  ];
  loading.replaceWith(
    el(
      "dl",
      { class: "properties" },
      rows.flatMap(([label, value]) => [
        el("dt", { text: label }),
        el("dd", {
          text: value ?? "—",
          class: value === null ? "muted" : "",
        }),
      ]),
    ),
  );
}
