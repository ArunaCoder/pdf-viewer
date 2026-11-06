//! Os comandos dos anexos: listar, ver, conferir, abrir e salvar.

use std::path::Path;
use std::sync::Arc;

use pdf_attachments::file_name::{extension, safe_file_name};
use pdf_attachments::kind::is_executable;
use pdf_attachments::{Attachment, AttachmentId, AttachmentIndex, ExtractLimits, Extracted, Integrity};
use tauri::ipc::Response;
use tauri::{AppHandle, Runtime};
use tauri_plugin_opener::OpenerExt as _;

use super::dto::{
    AttachmentDto, AttachmentListing, DocumentInfo, IntegrityDto, OpenFailure, SavedBatch, SavedFile, UnreadableDto,
};
use super::{blocking, file_dialog, state_of};
use crate::documents::{DocumentId, MAX_DOCUMENT_BYTES, OpenDocument};
use crate::error::{IpcError, IpcErrorCode};
use crate::fs_ops::{mark_from_internet, write_new, write_replacing};

/// O maior anexo que a prévia dentro do app recebe. A prévia de imagem e texto vive na
/// memória do webview; o arquivo maior que isso se salva ou se abre fora.
const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;

/// Os anexos do documento, lendo o índice na primeira chamada. `password` é a senha que o
/// usuário já deu ao pdf.js para desenhar o documento; o webview a repassa porque o índice
/// é lido por outro leitor, que também precisa dela.
#[tauri::command]
pub async fn list_attachments<R: Runtime>(
    document_id: DocumentId,
    password: Option<String>,
    app: AppHandle<R>,
) -> Result<AttachmentListing, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let index = document.attachments(password.as_deref())?;
        log::info!("document {document_id} has {} attachments", index.attachments().len());
        Ok(AttachmentListing {
            attachments: index.attachments().map(AttachmentDto::from).collect(),
            unreadable: index.unreadable().iter().map(UnreadableDto::from).collect(),
        })
    })
    .await
}

/// Os bytes do anexo para a prévia dentro do app.
#[tauri::command]
pub async fn attachment_bytes<R: Runtime>(
    document_id: DocumentId,
    attachment_id: u32,
    app: AppHandle<R>,
) -> Result<Response, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let (_, extracted) = extract(&document, attachment_id, MAX_PREVIEW_BYTES)?;
        Ok(Response::new(extracted.bytes))
    })
    .await
}

/// Confere o anexo contra o tamanho e o MD5 que o PDF declara.
#[tauri::command]
pub async fn verify_attachment<R: Runtime>(
    document_id: DocumentId,
    attachment_id: u32,
    app: AppHandle<R>,
) -> Result<IntegrityDto, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let (_, extracted) = extract(&document, attachment_id, ExtractLimits::default().max_bytes)?;
        Ok(extracted.integrity.into())
    })
    .await
}

/// Abre o anexo PDF numa aba nova, como documento próprio.
#[tauri::command]
pub async fn open_attachment_as_document<R: Runtime>(
    document_id: DocumentId,
    attachment_id: u32,
    app: AppHandle<R>,
) -> Result<DocumentInfo, IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let document = state.documents.get(document_id)?;
        let (attachment, extracted) = extract(&document, attachment_id, MAX_DOCUMENT_BYTES)?;
        let opened = state
            .documents
            .open_attachment(attachment.name, document_id, attachment.id, extracted.bytes)?;
        log::info!(
            "opened attachment {attachment_id} of document {document_id} as document {}",
            opened.id
        );
        Ok(DocumentInfo::from(opened.as_ref()))
    })
    .await
}

/// Salva o anexo onde o usuário escolher.
#[tauri::command]
pub async fn save_attachment<R: Runtime>(
    document_id: DocumentId,
    attachment_id: u32,
    app: AppHandle<R>,
) -> Result<Option<SavedFile>, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let index = read_index(&document)?;
        let attachment = find(&index, attachment_id)?;
        let name = safe_file_name(&attachment.name);
        let mut dialog = file_dialog(&app, "Salvar anexo").set_file_name(&name);
        if let Some(ext) = extension(&name) {
            dialog = dialog.add_filter(ext.to_uppercase(), &[ext.as_str()]);
        }
        let Some(path) = dialog
            .add_filter("Todos os arquivos", &["*"])
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
        else {
            return Ok(None);
        };
        let extracted = index.extract(attachment.id, ExtractLimits::default())?;
        warn_on_integrity(attachment, extracted.integrity);
        write_replacing(&path, &extracted.bytes).map_err(|error| IpcError::io("saving the attachment", &error))?;
        mark_from_internet(&path);
        Ok(Some(SavedFile {
            path: path.display().to_string(),
        }))
    })
    .await
}

/// Salva todos os anexos numa pasta, sem sobrescrever nada que já esteja nela.
#[tauri::command]
pub async fn save_all_attachments<R: Runtime>(
    document_id: DocumentId,
    app: AppHandle<R>,
) -> Result<Option<SavedBatch>, IpcError> {
    blocking(move || {
        let document = state_of(&app).documents.get(document_id)?;
        let index = read_index(&document)?;
        let Some(folder) = file_dialog(&app, "Salvar todos os anexos na pasta")
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
        else {
            return Ok(None);
        };
        Ok(Some(save_all_into(&index, &folder)))
    })
    .await
}

/// Grava cada anexo de `index` em `folder`; a falha de um não interrompe os outros.
fn save_all_into(index: &AttachmentIndex, folder: &Path) -> SavedBatch {
    let mut used: Vec<String> = Vec::new();
    let mut failed = Vec::new();
    for attachment in index.attachments() {
        let saved = index
            .extract(attachment.id, ExtractLimits::default())
            .map_err(IpcError::from)
            .and_then(|extracted| {
                warn_on_integrity(attachment, extracted.integrity);
                write_new(folder, &safe_file_name(&attachment.name), &extracted.bytes, &used)
                    .map_err(|error| IpcError::io("saving an attachment", &error))
            });
        match saved {
            Ok(path) => {
                mark_from_internet(&path);
                used.extend(path.file_name().map(|name| name.to_string_lossy().into_owned()));
            }
            Err(error) => failed.push(OpenFailure {
                name: attachment.name.clone(),
                error,
            }),
        }
    }
    SavedBatch {
        folder: folder.display().to_string(),
        saved: used.len(),
        failed,
    }
}

/// Abre o anexo com o programa padrão do sistema, a partir de uma cópia na pasta própria do
/// app. Executável é recusado: o anexo é do autor do PDF, e abrir um `.exe` com um clique
/// é executá-lo.
#[tauri::command]
pub async fn open_attachment_externally<R: Runtime>(
    document_id: DocumentId,
    attachment_id: u32,
    app: AppHandle<R>,
) -> Result<(), IpcError> {
    blocking(move || {
        let state = state_of(&app);
        let document = state.documents.get(document_id)?;
        let index = read_index(&document)?;
        let attachment = find(&index, attachment_id)?;
        let name = safe_file_name(&attachment.name);
        if is_executable(&name) {
            return Err(IpcError::new(
                IpcErrorCode::BlockedExecutable,
                format!("{name} is executable"),
            ));
        }
        let extracted = index.extract(attachment.id, ExtractLimits::default())?;
        warn_on_integrity(attachment, extracted.integrity);
        // Uma pasta por anexo: dois anexos de mesmo nome não se sobrescrevem abertos.
        let dir = state.opened_dir.join(format!("{document_id}-{attachment_id}"));
        let path = dir.join(&name);
        std::fs::create_dir_all(&dir)
            .map_err(|error| IpcError::io("preparing the opened attachments folder", &error))?;
        write_replacing(&path, &extracted.bytes)
            .map_err(|error| IpcError::io("writing the attachment to open", &error))?;
        mark_from_internet(&path);
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|error| IpcError::new(IpcErrorCode::Io, format!("opening the attachment: {error}")))
    })
    .await
}

/// O índice já lido do documento, ou o que dá para ler sem senha.
fn read_index(document: &OpenDocument) -> Result<Arc<AttachmentIndex>, IpcError> {
    match document.attachments_if_read() {
        Some(index) => Ok(index),
        None => Ok(document.attachments(None)?),
    }
}

fn find(index: &AttachmentIndex, attachment_id: u32) -> Result<&Attachment, IpcError> {
    index.get(AttachmentId::new(attachment_id)).ok_or_else(|| {
        IpcError::new(
            IpcErrorCode::UnknownAttachment,
            format!("there is no attachment {attachment_id}"),
        )
    })
}

/// O anexo e os bytes dele, extraídos com o teto `max_bytes`.
fn extract(document: &OpenDocument, attachment_id: u32, max_bytes: u64) -> Result<(Attachment, Extracted), IpcError> {
    let index = read_index(document)?;
    let attachment = find(&index, attachment_id)?.clone();
    let extracted = index.extract(attachment.id, ExtractLimits { max_bytes })?;
    Ok((attachment, extracted))
}

/// A conferência que falhou não impede salvar nem abrir — o arquivo é do usuário —, mas
/// fica no log, que é onde se investiga um anexo adulterado.
fn warn_on_integrity(attachment: &Attachment, integrity: Integrity) {
    if matches!(integrity, Integrity::SizeMismatch { .. } | Integrity::ChecksumMismatch) {
        log::warn!(
            "attachment {} does not match its declared metadata: {integrity:?}",
            attachment.id
        );
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use lopdf::{Document, Object, Stream, StringFormat, dictionary};
    use pdf_attachments::AttachmentIndex;

    use super::save_all_into;

    type TestResult = Result<(), Box<dyn Error>>;

    /// Um PDF com anexos de nomes dados, todos na árvore de nomes.
    fn pdf_with(files: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut doc = Document::with_version("1.7");
        let pages = doc.add_object(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 });
        let mut names = Vec::new();
        for (name, content) in files {
            let stream = doc.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, content.to_vec()));
            let text = Object::String(name.as_bytes().to_vec(), StringFormat::Literal);
            let spec = doc.add_object(
                dictionary! { "Type" => "Filespec", "F" => text.clone(), "EF" => dictionary! { "F" => stream } },
            );
            names.push(text);
            names.push(spec.into());
        }
        let tree = doc.add_object(dictionary! { "Names" => names });
        let catalog = doc.add_object(
            dictionary! { "Type" => "Catalog", "Pages" => pages, "Names" => dictionary! { "EmbeddedFiles" => tree } },
        );
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)?;
        Ok(bytes)
    }

    #[test]
    fn saving_all_keeps_hostile_names_inside_the_folder_and_apart() -> TestResult {
        let index = AttachmentIndex::open(
            &pdf_with(&[
                ("..\\..\\fuga.txt", b"1"),
                ("nota.xml", b"2"),
                ("NOTA.xml", b"3"),
                ("CON", b"4"),
            ])?,
            None,
        )?;
        let parent = tempfile::tempdir()?;
        let folder = parent.path().join("destino");
        fs::create_dir(&folder)?;
        fs::write(folder.join("nota.xml"), b"do usuario")?;

        let batch = save_all_into(&index, &folder);
        assert_eq!(batch.saved, 4);
        assert!(batch.failed.is_empty());

        let mut saved: Vec<String> = fs::read_dir(&folder)?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        saved.sort();
        // O NTFS não distingue caixa: `NOTA.xml` colide com o `nota.xml` do usuário, e o (2) já
        // é do lote.
        assert_eq!(saved, ["NOTA (3).xml", "_CON", "fuga.txt", "nota (2).xml", "nota.xml"]);
        assert_eq!(
            fs::read(folder.join("nota.xml"))?,
            b"do usuario",
            "nada que já estava foi sobrescrito"
        );
        assert_eq!(
            fs::read_dir(parent.path())?.count(),
            1,
            "nenhum arquivo escapou da pasta escolhida"
        );
        Ok(())
    }
}
