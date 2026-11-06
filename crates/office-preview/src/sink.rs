//! Onde os leitores dos dois formatos depositam o que leem: aqui ficam os tetos de texto,
//! de blocos e de profundidade, e as imagens, conferidas e sem repetição.

use std::collections::HashMap;

use crate::archive::Archive;
use crate::{Block, Format, Image, ImageType, Inline, Limits, Marks, Notes, Preview};

/// A prévia em construção.
pub struct Sink {
    /// Os avisos, que os leitores também alimentam.
    pub notes: Notes,
    images: Vec<Image>,
    by_path: HashMap<String, usize>,
    text_left: usize,
    blocks_left: usize,
    max_depth: usize,
    max_images: usize,
    max_image_bytes: u64,
}

impl Sink {
    pub fn new(limits: &Limits) -> Self {
        Self {
            notes: Notes::default(),
            images: Vec::new(),
            by_path: HashMap::new(),
            text_left: limits.max_text_chars,
            blocks_left: limits.max_blocks,
            max_depth: limits.max_depth,
            max_images: limits.max_images,
            max_image_bytes: limits.max_image_bytes,
        }
    }

    /// Se a travessia ainda pode descer a `depth`; abaixo do teto, o conteúdo fica de fora e
    /// a prévia sai marcada como cortada.
    pub const fn descend(&mut self, depth: usize) -> bool {
        if depth > self.max_depth {
            self.notes.truncated = true;
            return false;
        }
        true
    }

    /// Reserva um bloco; `false` quando o teto de blocos acabou.
    pub const fn take_block(&mut self) -> bool {
        if let Some(left) = self.blocks_left.checked_sub(1) {
            self.blocks_left = left;
            true
        } else {
            self.notes.truncated = true;
            false
        }
    }

    /// Acrescenta `text` a `out`, até onde o teto de texto deixa. Trecho com as mesmas
    /// marcas do anterior se junta a ele.
    pub fn push_text(&mut self, out: &mut Vec<Inline>, text: &str, marks: Marks) {
        if text.is_empty() {
            return;
        }
        let fitting: String = text.chars().take(self.text_left).collect();
        let taken = fitting.chars().count();
        self.text_left -= taken;
        if taken < text.chars().count() {
            self.notes.truncated = true;
        }
        if fitting.is_empty() {
            return;
        }
        if let Some(Inline::Text {
            text: last,
            marks: last_marks,
        }) = out.last_mut()
            && *last_marks == marks
        {
            last.push_str(&fitting);
            return;
        }
        out.push(Inline::Text { text: fitting, marks });
    }

    /// Acrescenta uma quebra de linha, que conta como um caractere.
    pub fn push_break(&mut self, out: &mut Vec<Inline>) {
        if let Some(left) = self.text_left.checked_sub(1) {
            self.text_left = left;
            out.push(Inline::Break);
        } else {
            self.notes.truncated = true;
        }
    }

    /// A posição da imagem da parte `path`, lida e conferida na primeira citação.
    ///
    /// `None` é a imagem que não entra — ausente, ilegível, acima do teto ou de formato
    /// que o navegador não desenha. Quem conta o aviso é quem chama: no `.odt` a mesma
    /// moldura oferece alternativas, e só a falta de todas é uma imagem perdida.
    pub fn image(&mut self, archive: &mut Archive<'_>, path: &str) -> Option<usize> {
        if let Some(&index) = self.by_path.get(path) {
            return Some(index);
        }
        if self.images.len() >= self.max_images {
            return None;
        }
        let bytes = archive.read(path, self.max_image_bytes).ok().flatten()?;
        let kind = ImageType::sniff(&bytes)?;
        let index = self.images.len();
        self.images.push(Image { kind, bytes });
        self.by_path.insert(path.to_owned(), index);
        Some(index)
    }

    /// Conta uma imagem que ficou de fora.
    pub const fn skip_image(&mut self) {
        self.notes.skipped_images = self.notes.skipped_images.saturating_add(1);
    }

    /// Conta um objeto que a prévia não desenha.
    pub const fn skip_object(&mut self) {
        self.notes.skipped_objects = self.notes.skipped_objects.saturating_add(1);
    }

    /// A prévia pronta.
    pub fn finish(self, format: Format, blocks: Vec<Block>) -> Preview {
        Preview {
            format,
            blocks,
            images: self.images,
            notes: self.notes,
        }
    }
}

/// Se o último texto de `out` termina em espaço — ou se não há texto antes, o começo do
/// parágrafo, onde espaço também não conta. O ODF junta espaços como o HTML.
pub fn ends_in_space(out: &[Inline]) -> bool {
    match out.last() {
        Some(Inline::Text { text, .. }) => text.ends_with(' '),
        Some(Inline::Image(_)) => false,
        Some(Inline::Break) | None => true,
    }
}
