//! A busca das especificações de arquivo (`/Filespec`) pelos três caminhos do PDF.
//!
//! Aqui só se **encontra**: cada achado é a especificação, a origem e o que o caminho
//! acrescenta a ela (a chave da árvore de nomes, o texto da anotação). Ler o arquivo que a
//! especificação descreve é de [`crate::filespec`].

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::index::Origin;
use crate::text::display_text;

/// A profundidade máxima da árvore de nomes. A especificação não fixa uma, e um leitor
/// recursivo sem teto é o alvo de quem constrói o PDF para estourar a pilha; árvores reais
/// têm dois ou três níveis.
const MAX_NAME_TREE_DEPTH: usize = 32;

/// Uma especificação de arquivo encontrada, com o que o caminho que a encontrou diz dela.
pub struct Found<'a> {
    pub filespec: &'a Dictionary,
    pub origin: Origin,
    /// A chave da árvore de nomes, que é o nome do anexo quando a especificação não tem um.
    pub tree_key: Option<String>,
    /// O `/Contents` da anotação, que é a descrição quando a especificação não tem uma.
    pub annotation_text: Option<String>,
}

/// Todas as especificações de arquivo do documento, na ordem em que um leitor as mostra:
/// primeiro as do documento (árvore de nomes, depois `/AF` do catálogo), depois as das
/// páginas, página a página.
pub fn find_filespecs(document: &Document) -> Vec<Found<'_>> {
    let mut found = Vec::new();
    if let Ok(catalog) = document.catalog() {
        if let Some(tree) =
            resolve_dict(document, catalog.get(b"Names").ok()).and_then(|names| names.get(b"EmbeddedFiles").ok())
        {
            let mut visited = HashSet::new();
            walk_name_tree(document, tree, 0, &mut visited, &mut found);
        }
        push_associated_files(document, catalog, Origin::Document, &mut found);
    }
    for (number, page_id) in document.get_pages() {
        let origin = Origin::Page { number };
        push_file_annotations(document, page_id, origin, &mut found);
        if let Ok(page) = document.get_dictionary(page_id) {
            push_associated_files(document, page, origin, &mut found);
        }
    }
    found
}

/// Percorre um nó da árvore de nomes: as folhas em `/Names` (pares chave–valor) e os
/// filhos em `/Kids`. Nó já visitado é ciclo e não se repete.
fn walk_name_tree<'a>(
    document: &'a Document,
    node: &'a Object,
    depth: usize,
    visited: &mut HashSet<ObjectId>,
    found: &mut Vec<Found<'a>>,
) {
    if depth > MAX_NAME_TREE_DEPTH {
        return;
    }
    let Ok((node_id, node)) = document.dereference(node) else {
        return;
    };
    if node_id.is_some_and(|id| !visited.insert(id)) {
        return;
    }
    let Ok(node) = node.as_dict() else { return };

    if let Some(names) = resolve_array(document, node.get(b"Names").ok()) {
        for pair in names.chunks_exact(2) {
            let [key, value] = pair else { continue };
            if let Some(filespec) = resolve_dict(document, Some(value)) {
                found.push(Found {
                    filespec,
                    origin: Origin::Document,
                    tree_key: display_text(document, key),
                    annotation_text: None,
                });
            }
        }
    }
    if let Some(kids) = resolve_array(document, node.get(b"Kids").ok()) {
        for kid in kids {
            walk_name_tree(document, kid, depth + 1, visited, found);
        }
    }
}

/// As anotações `/FileAttachment` de uma página.
fn push_file_annotations<'a>(document: &'a Document, page_id: ObjectId, origin: Origin, found: &mut Vec<Found<'a>>) {
    let Ok(annotations) = document.get_page_annotations(page_id) else {
        return;
    };
    for annotation in annotations {
        let is_file_attachment = annotation
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"FileAttachment");
        if !is_file_attachment {
            continue;
        }
        if let Some(filespec) = resolve_dict(document, annotation.get(b"FS").ok()) {
            let annotation_text = annotation
                .get(b"Contents")
                .ok()
                .and_then(|text| display_text(document, text));
            found.push(Found {
                filespec,
                origin,
                tree_key: None,
                annotation_text,
            });
        }
    }
}

/// A lista `/AF` de arquivos associados de um dicionário (catálogo ou página).
fn push_associated_files<'a>(
    document: &'a Document,
    owner: &'a Dictionary,
    origin: Origin,
    found: &mut Vec<Found<'a>>,
) {
    let Some(files) = resolve_array(document, owner.get(b"AF").ok()) else {
        return;
    };
    for entry in files {
        if let Some(filespec) = resolve_dict(document, Some(entry)) {
            found.push(Found {
                filespec,
                origin,
                tree_key: None,
                annotation_text: None,
            });
        }
    }
}

/// O dicionário que `object` é, ou para o qual aponta.
pub fn resolve_dict<'a>(document: &'a Document, object: Option<&'a Object>) -> Option<&'a Dictionary> {
    document.dereference(object?).ok()?.1.as_dict().ok()
}

/// O array que `object` é, ou para o qual aponta.
fn resolve_array<'a>(document: &'a Document, object: Option<&'a Object>) -> Option<&'a [Object]> {
    document.dereference(object?).ok()?.1.as_array().ok().map(Vec::as_slice)
}
