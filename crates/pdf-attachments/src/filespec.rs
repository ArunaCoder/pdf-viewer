//! A leitura de uma especificação de arquivo: o nome, a descrição, os parâmetros e o
//! stream que guarda os bytes (PDF 32000-1 §7.11.3 e §7.11.4).

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::collect::{Found, resolve_dict};
use crate::date::PdfDate;
use crate::index::UnreadableReason;
use crate::text::display_text;

/// As chaves de nome da especificação, na ordem de preferência: `/UF` é a única em Unicode
/// garantido; `/F` é a de compatibilidade; as três últimas são de plataforma, obsoletas
/// desde o PDF 1.7, e só aparecem em arquivo antigo.
const NAME_KEYS: [&[u8]; 5] = [b"UF", b"F", b"Unix", b"Mac", b"DOS"];

/// As chaves do `/EF` que apontam para o stream embutido, na mesma preferência.
const STREAM_KEYS: [&[u8]; 5] = [b"UF", b"F", b"Unix", b"Mac", b"DOS"];

/// Tudo o que a especificação diz do anexo, ainda sem o id que o índice atribui.
pub struct Draft {
    pub stream: Stream,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub declared_size: Option<u64>,
    pub created: Option<PdfDate>,
    pub modified: Option<PdfDate>,
    pub checksum: Option<[u8; 16]>,
    pub relationship: Option<String>,
}

/// O nome que a especificação dá ao arquivo, sem caminho, ou a chave da árvore de nomes.
pub fn name_of(document: &Document, found: &Found<'_>) -> Option<String> {
    NAME_KEYS
        .iter()
        .find_map(|key| {
            found
                .filespec
                .get(key)
                .ok()
                .and_then(|value| display_text(document, value))
        })
        .map(|name| last_component(&name).to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| found.tree_key.clone())
}

/// O id do stream que guarda os bytes do arquivo — a identidade do anexo. A especificação
/// sem `/EF` é de arquivo **externo** (o PDF só cita um caminho); com `/EF` mas sem
/// referência a stream, está quebrada.
pub fn embedded_stream_id(document: &Document, found: &Found<'_>) -> Result<ObjectId, UnreadableReason> {
    let embedded = resolve_dict(document, found.filespec.get(b"EF").ok()).ok_or(UnreadableReason::NotEmbedded)?;
    STREAM_KEYS
        .iter()
        .find_map(|key| embedded.get(key).and_then(Object::as_reference).ok())
        .ok_or(UnreadableReason::MissingStream)
}

/// Lê a especificação e o stream `stream_id` que [`embedded_stream_id`] achou nela.
pub fn read(document: &Document, found: &Found<'_>, stream_id: ObjectId) -> Result<Draft, UnreadableReason> {
    let filespec = found.filespec;
    let stream = document
        .get_object(stream_id)
        .and_then(Object::as_stream)
        .map_err(|_| UnreadableReason::MissingStream)?
        .clone();

    let params = resolve_dict(document, stream.dict.get(b"Params").ok());
    let date = |key: &[u8]| {
        params
            .and_then(|params| text_value(document, params, key))
            .and_then(|text| PdfDate::parse(&text))
    };

    Ok(Draft {
        name: name_of(document, found),
        description: text_value(document, filespec, b"Desc").or_else(|| found.annotation_text.clone()),
        mime_type: name_value(&stream.dict, b"Subtype").filter(|mime| mime.contains('/')),
        declared_size: params
            .and_then(|params| params.get(b"Size").and_then(Object::as_i64).ok())
            .and_then(|size| u64::try_from(size).ok()),
        created: date(b"CreationDate"),
        modified: date(b"ModDate"),
        checksum: params.and_then(|params| checksum(document, params)),
        relationship: name_value(filespec, b"AFRelationship"),
        stream,
    })
}

/// O texto de `key` em `dict`, decodificado para exibição.
fn text_value(document: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|value| display_text(document, value))
}

/// O nome PDF de `key` em `dict`, como texto. O parser já desfez os escapes `#2F`.
fn name_value(dict: &Dictionary, key: &[u8]) -> Option<String> {
    let name = dict.get(key).and_then(Object::as_name).ok()?;
    let text = String::from_utf8_lossy(name);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// O MD5 declarado em `/CheckSum`: uma string de exatamente 16 bytes, ou nada.
fn checksum(document: &Document, params: &Dictionary) -> Option<[u8; 16]> {
    let value = document.dereference(params.get(b"CheckSum").ok()?).ok()?.1;
    value.as_str().ok()?.try_into().ok()
}

/// O último componente de um caminho de especificação de arquivo, que separa com `/` e,
/// nos produtores Windows, com `\`.
fn last_component(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name).trim()
}
