// Formatação para exibição, em pt-BR. Funções puras: a suíte do Vitest as cobre.

import type { Attachment, Origin } from "./ipc";

const UNITS = ["B", "KB", "MB", "GB"] as const;

const decimal = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });

/** Um tamanho em bytes na maior unidade que o deixa abaixo de 1024. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? String(value) : decimal.format(value);
  return `${digits} ${UNITS[unit]}`;
}

const dateTime = new Intl.DateTimeFormat("pt-BR", {
  dateStyle: "short",
  timeStyle: "short",
});

/**
 * Uma data ISO 8601 do núcleo, em data e hora locais. Sem fuso declarado, a data é a do
 * relógio de quem gerou o PDF, e é exibida como está, sem conversão.
 */
export function formatIsoDate(iso: string | null): string | null {
  if (iso === null) return null;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : dateTime.format(date);
}

/** Uma data já em `Date`, em data e hora locais. */
export function formatDate(date: Date | null): string | null {
  return date === null || Number.isNaN(date.getTime()) ? null : dateTime.format(date);
}

/** `1 anexo`, `3 anexos`. */
export function countLabel(count: number, singular: string, plural: string): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

/** Onde o anexo está preso, para a linha de metadados. */
export function originLabel(origin: Origin): string {
  return origin.kind === "document" ? "Documento" : `Página ${origin.number}`;
}

const KIND_LABELS: Record<Attachment["kind"], string> = {
  pdf: "PDF",
  image: "Imagem",
  text: "Texto",
  other: "Arquivo",
};

/** O tipo do anexo para exibir: a extensão em maiúsculas, ou o rótulo do tipo. */
export function typeLabel(attachment: Attachment): string {
  const dot = attachment.name.lastIndexOf(".");
  const extension = dot > 0 ? attachment.name.slice(dot + 1).toUpperCase() : "";
  return extension.length > 0 && extension.length <= 6 ? extension : KIND_LABELS[attachment.kind];
}

/** A linha de metadados de um anexo: tipo · tamanho · origem. */
export function attachmentMeta(attachment: Attachment): string {
  const parts = [typeLabel(attachment)];
  if (attachment.declaredSize !== null) {
    parts.push(formatBytes(attachment.declaredSize));
  }
  parts.push(originLabel(attachment.origin));
  return parts.join(" · ");
}

/** O zoom em porcentagem inteira. */
export function formatZoom(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}
