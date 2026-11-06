//! Montagem de PDFs de teste em memória — cada teste constrói o arquivo que descreve, e o
//! valor esperado mora no próprio teste, ao lado da montagem.

use std::error::Error;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};

pub type TestResult = Result<(), Box<dyn Error>>;

/// Os metadados opcionais de um arquivo embutido.
#[derive(Default)]
pub struct FileParams<'a> {
    pub mime: Option<&'a str>,
    pub size: Option<i64>,
    pub checksum: Option<[u8; 16]>,
    pub created: Option<&'a str>,
    pub modified: Option<&'a str>,
    pub compress: bool,
}

pub struct PdfBuilder {
    doc: Document,
    pages_id: ObjectId,
    page_ids: Vec<ObjectId>,
    tree_entries: Vec<Object>,
    catalog_extra: Dictionary,
}

/// Uma string literal com os bytes de `text` — `PDFDocEncoding`, então só ASCII é fiel.
pub fn literal(text: &str) -> Object {
    Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
}

pub fn utf16(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

impl PdfBuilder {
    pub fn with_pages(count: usize) -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_ids = (0..count)
            .map(|_| {
                doc.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                })
            })
            .collect();
        Self {
            doc,
            pages_id,
            page_ids,
            tree_entries: Vec::new(),
            catalog_extra: Dictionary::new(),
        }
    }

    pub const fn doc_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    /// Um stream de arquivo embutido (`/Type /EmbeddedFile`).
    pub fn embedded_stream(&mut self, content: &[u8], params: &FileParams<'_>) -> ObjectId {
        let mut param_dict = Dictionary::new();
        if let Some(size) = params.size {
            param_dict.set("Size", size);
        }
        if let Some(checksum) = params.checksum {
            param_dict.set("CheckSum", Object::String(checksum.to_vec(), StringFormat::Hexadecimal));
        }
        if let Some(created) = params.created {
            param_dict.set("CreationDate", literal(created));
        }
        if let Some(modified) = params.modified {
            param_dict.set("ModDate", literal(modified));
        }
        let mut dict = dictionary! { "Type" => "EmbeddedFile", "Params" => param_dict };
        if let Some(mime) = params.mime {
            dict.set("Subtype", Object::Name(mime.as_bytes().to_vec()));
        }
        let mut stream = Stream::new(dict, content.to_vec());
        if params.compress {
            stream.compress().ok();
        }
        self.doc.add_object(stream)
    }

    /// Uma especificação de arquivo com as entradas dadas e `/EF` apontando para `stream`.
    pub fn filespec(&mut self, entries: Dictionary, stream: Option<ObjectId>) -> ObjectId {
        let mut dict = dictionary! { "Type" => "Filespec" };
        for (key, value) in entries {
            dict.set(key, value);
        }
        if let Some(stream) = stream {
            dict.set("EF", dictionary! { "F" => stream, "UF" => stream });
        }
        self.doc.add_object(dict)
    }

    /// Um arquivo completo — stream mais especificação com `/UF` — na árvore de nomes.
    pub fn attach(&mut self, name: &str, content: &[u8], params: &FileParams<'_>) -> ObjectId {
        let stream = self.embedded_stream(content, params);
        let filespec = self.filespec(dictionary! { "UF" => utf16(name), "F" => literal(name) }, Some(stream));
        self.add_to_tree(name, filespec);
        filespec
    }

    pub fn add_to_tree(&mut self, key: &str, filespec: ObjectId) {
        self.tree_entries.push(literal(key));
        self.tree_entries.push(filespec.into());
    }

    /// Uma anotação `/FileAttachment` na página `page` (a partir de 1).
    pub fn annotate(&mut self, page: usize, filespec: ObjectId, contents: Option<Object>) {
        let mut annotation = dictionary! {
            "Type" => "Annot",
            "Subtype" => "FileAttachment",
            "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
            "FS" => filespec,
        };
        if let Some(contents) = contents {
            annotation.set("Contents", contents);
        }
        let annotation_id = self.doc.add_object(annotation);
        let Some(&page_id) = self.page_ids.get(page - 1) else {
            return;
        };
        if let Ok(page) = self.doc.get_dictionary_mut(page_id) {
            page.set("Annots", vec![annotation_id.into()]);
        }
    }

    pub fn set_catalog(&mut self, key: &str, value: impl Into<Object>) {
        self.catalog_extra.set(key, value);
    }

    /// O documento montado, ainda em memória (para cifrar antes de gravar).
    pub fn finish(mut self) -> Document {
        let kids: Vec<Object> = self.page_ids.iter().map(|&id| id.into()).collect();
        let count = i64::try_from(kids.len()).unwrap_or(i64::MAX);
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => self.pages_id };
        if !self.tree_entries.is_empty() {
            let tree = self.doc.add_object(dictionary! { "Names" => self.tree_entries });
            catalog.set("Names", dictionary! { "EmbeddedFiles" => tree });
        }
        for (key, value) in self.catalog_extra {
            catalog.set(key, value);
        }
        let catalog_id = self.doc.add_object(catalog);
        self.doc.trailer.set("Root", catalog_id);
        let file_id = Object::String(b"0123456789abcdef".to_vec(), StringFormat::Hexadecimal);
        self.doc.trailer.set("ID", vec![file_id.clone(), file_id]);
        self.doc
    }

    pub fn build(self) -> Result<Vec<u8>, Box<dyn Error>> {
        save(&mut self.finish())
    }
}

pub fn save(doc: &mut Document) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}
