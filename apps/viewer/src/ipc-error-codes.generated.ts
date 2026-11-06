// Gerado por `error::tests::generated_ts_matches_the_enum` (src-tauri/src/error.rs).
// Não edite: rode `REGENERATE=1 cargo test -p pdf-viewer generated_ts` após mudar o enum.

export type IpcErrorCode =
  | "unknown_document"
  | "unknown_attachment"
  | "file_not_found"
  | "access_denied"
  | "io"
  | "not_a_pdf"
  | "document_too_large"
  | "password_required"
  | "wrong_password"
  | "malformed_pdf"
  | "attachment_too_large"
  | "attachment_undecodable"
  | "blocked_executable"
  | "unsupported_link"
  | "not_on_disk"
  | "unknown_recent"
  | "internal";
