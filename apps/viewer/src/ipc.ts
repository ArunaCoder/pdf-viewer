// A fronteira com o Rust: os comandos tipados e a tradução dos erros para português.
//
// Os tipos espelham os DTOs de `src-tauri/src/ipc/dto.rs`, cuja forma do fio é travada lá
// por teste contra o JSON literal. O webview não decide nada sobre o anexo — se é
// executável, que tipo tem, se confere: recebe o veredito do núcleo e o mostra.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { IpcErrorCode } from "./ipc-error-codes.generated";

export type { IpcErrorCode };

export interface DocumentInfo {
  id: number;
  name: string;
  size: number;
  path: string | null;
  parentId: number | null;
}

export interface IpcErrorPayload {
  code: IpcErrorCode;
  message: string;
}

export interface OpenFailure {
  name: string;
  error: IpcErrorPayload;
}

export interface OpenOutcome {
  opened: DocumentInfo[];
  failed: OpenFailure[];
}

export type Origin = { kind: "document" } | { kind: "page"; number: number };

export type AttachmentKind = "pdf" | "image" | "text" | "document" | "other";

export interface Attachment {
  id: number;
  name: string;
  description: string | null;
  mimeType: string | null;
  declaredSize: number | null;
  created: string | null;
  modified: string | null;
  hasChecksum: boolean;
  relationship: string | null;
  origin: Origin;
  kind: AttachmentKind;
  executable: boolean;
}

export interface Unreadable {
  name: string | null;
  origin: Origin;
  reason: "not_embedded" | "missing_stream";
}

export interface AttachmentListing {
  attachments: Attachment[];
  unreadable: Unreadable[];
}

export type Integrity =
  | { status: "verified" }
  | { status: "not_declared" }
  | { status: "size_mismatch"; declared: number; actual: number }
  | { status: "checksum_mismatch" };

/** A prévia de leitura de um `.docx` ou `.odt`: blocos com tipo, montados pelo Rust. */
export interface DocumentPreview {
  format: "docx" | "odt";
  blocks: DocumentBlock[];
  /** O MIME saiu da assinatura dos bytes, no Rust; `data` é base64. */
  images: Array<{ mimeType: string; data: string }>;
  notes: DocumentNotes;
}

export type DocumentBlock =
  | {
      type: "paragraph";
      role: DocumentRole;
      marker: string | null;
      align: "start" | "center" | "end" | "justify";
      inlines: DocumentInline[];
    }
  | { type: "table"; rows: Array<{ cells: DocumentCell[] }> };

export type DocumentRole =
  | { kind: "body" }
  | { kind: "heading"; level: number }
  | { kind: "list_item"; level: number };

export type DocumentMark = "bold" | "italic" | "underline" | "strike";

export type DocumentInline =
  | { type: "text"; text: string; marks: DocumentMark[] }
  | { type: "break" }
  | { type: "image"; index: number };

export interface DocumentCell {
  colSpan: number;
  rowSpan: number;
  blocks: DocumentBlock[];
}

export interface DocumentNotes {
  hasMacros: boolean;
  skippedImages: number;
  skippedObjects: number;
  truncated: boolean;
}

export interface RecentEntry {
  name: string;
  folder: string;
}

export interface SavedFile {
  path: string;
}

export interface SavedBatch {
  folder: string;
  saved: number;
  failed: OpenFailure[];
}

/** A falha de um comando, com o código que decide o que mostrar. */
export class IpcFailure extends Error {
  constructor(
    readonly code: IpcErrorCode,
    readonly detail: string,
  ) {
    super(errorMessage(code));
    this.name = "IpcFailure";
  }
}

function isErrorPayload(value: unknown): value is IpcErrorPayload {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { code?: unknown }).code === "string" &&
    typeof (value as { message?: unknown }).message === "string"
  );
}

/** A falha do Rust como `IpcFailure`; o que não vier na forma do contrato é interno. */
export function toFailure(error: unknown): IpcFailure {
  if (error instanceof IpcFailure) return error;
  if (isErrorPayload(error)) return new IpcFailure(error.code, error.message);
  return new IpcFailure("internal", String(error));
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toFailure(error);
  }
}

/** O texto em português para o código. */
export function errorMessage(code: IpcErrorCode): string {
  switch (code) {
    case "unknown_document":
      return "Este documento não está mais aberto.";
    case "unknown_attachment":
      return "Este anexo não existe mais no documento.";
    case "file_not_found":
      return "O arquivo não foi encontrado — ele pode ter sido movido ou apagado.";
    case "access_denied":
      return "O Windows negou acesso ao arquivo.";
    case "io":
      return "Não foi possível ler ou gravar o arquivo.";
    case "not_a_pdf":
      return "O arquivo não é um PDF.";
    case "document_too_large":
      return "O PDF é grande demais para abrir (limite de 1 GB).";
    case "password_required":
      return "O PDF é protegido por senha.";
    case "wrong_password":
      return "A senha está incorreta.";
    case "malformed_pdf":
      return "A estrutura do PDF está danificada; os anexos não puderam ser lidos.";
    case "attachment_too_large":
      return "O anexo é grande demais para esta operação.";
    case "attachment_undecodable":
      return "O conteúdo do anexo está corrompido ou usa uma compressão não suportada.";
    case "unsupported_document":
      return "O conteúdo do anexo não é um documento .docx ou .odt, apesar do nome.";
    case "protected_document":
      return "O documento é protegido por senha ou está num formato antigo (.doc); não há pré-visualização.";
    case "malformed_document":
      return "A estrutura do documento está danificada; não foi possível pré-visualizá-lo.";
    case "blocked_executable":
      return "Arquivos executáveis não são abertos pelo visualizador. Salve o arquivo e verifique-o antes de usar.";
    case "unsupported_link":
      return "Este tipo de link não é aberto fora do visualizador, por segurança.";
    case "not_on_disk":
      return "Este documento veio de um anexo e não está em nenhuma pasta. Salve uma cópia primeiro.";
    case "unknown_recent":
      return "Este item não está mais na lista de recentes.";
    case "internal":
      return "Ocorreu um erro interno no visualizador.";
    default:
      return unhandled(code);
  }
}

function unhandled(code: never): string {
  return `Erro inesperado (${String(code)}).`;
}

// ── Comandos ────────────────────────────────────────────────────────────────

export const pickDocuments = (): Promise<OpenOutcome> => call("pick_documents");

export const takePendingDocuments = (): Promise<OpenOutcome> => call("take_pending_documents");

export const recentDocuments = (): Promise<RecentEntry[]> => call("recent_documents");

export const openRecent = (index: number): Promise<DocumentInfo> => call("open_recent", { index });

export const clearRecent = (): Promise<void> => call("clear_recent");

export const documentBytes = (documentId: number): Promise<ArrayBuffer> =>
  call("document_bytes", { documentId });

export const closeDocument = (documentId: number): Promise<void> =>
  call("close_document", { documentId });

export const saveDocumentCopy = (documentId: number): Promise<SavedFile | null> =>
  call("save_document_copy", { documentId });

export const showInFolder = (documentId: number): Promise<void> =>
  call("show_in_folder", { documentId });

export const openExternalLink = (url: string): Promise<void> => call("open_external_link", { url });

export const listAttachments = (
  documentId: number,
  password: string | null,
): Promise<AttachmentListing> => call("list_attachments", { documentId, password });

export const attachmentBytes = (documentId: number, attachmentId: number): Promise<ArrayBuffer> =>
  call("attachment_bytes", { documentId, attachmentId });

export const attachmentDocumentPreview = (
  documentId: number,
  attachmentId: number,
): Promise<DocumentPreview> => call("attachment_document_preview", { documentId, attachmentId });

export const verifyAttachment = (documentId: number, attachmentId: number): Promise<Integrity> =>
  call("verify_attachment", { documentId, attachmentId });

export const openAttachmentAsDocument = (
  documentId: number,
  attachmentId: number,
): Promise<DocumentInfo> => call("open_attachment_as_document", { documentId, attachmentId });

export const saveAttachment = (
  documentId: number,
  attachmentId: number,
): Promise<SavedFile | null> => call("save_attachment", { documentId, attachmentId });

export const saveAllAttachments = (documentId: number): Promise<SavedBatch | null> =>
  call("save_all_attachments", { documentId });

export const openAttachmentExternally = (documentId: number, attachmentId: number): Promise<void> =>
  call("open_attachment_externally", { documentId, attachmentId });

/** Os documentos que o Rust abriu por fora do webview (arrastar, segunda instância). */
export const onDocumentsOpened = (handler: (outcome: OpenOutcome) => void): Promise<UnlistenFn> =>
  listen<OpenOutcome>("documents-opened", (event) => handler(event.payload));
