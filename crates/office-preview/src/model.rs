//! A prévia montada: blocos com tipo, sem nada do arquivo que precise ser interpretado.

/// O formato de onde a prévia saiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Word (`WordprocessingML`, ECMA-376).
    Docx,
    /// `OpenDocument` Texto (ISO/IEC 26300).
    Odt,
}

/// O documento pronto para desenhar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// O formato reconhecido pelo conteúdo.
    pub format: Format,
    /// O corpo do documento, em ordem.
    pub blocks: Vec<Block>,
    /// As imagens que os blocos citam por posição ([`Inline::Image`]).
    pub images: Vec<Image>,
    /// O que ficou de fora ou merece aviso.
    pub notes: Notes,
}

/// Um bloco do corpo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Um parágrafo, título ou item de lista.
    Paragraph(Paragraph),
    /// Uma tabela.
    Table(Table),
}

/// Um parágrafo e o papel dele no documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paragraph {
    /// Corpo, título ou item de lista.
    pub role: Role,
    /// O marcador já formatado (`1.`, `a)`, `•`) do item de lista ou do título numerado.
    pub marker: Option<String>,
    /// O alinhamento declarado no próprio parágrafo ou no estilo dele.
    pub align: Align,
    /// O conteúdo.
    pub inlines: Vec<Inline>,
}

/// O papel de um parágrafo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Texto corrido.
    Body,
    /// Um título, de nível 1 a 6.
    Heading(u8),
    /// Um item de lista, com o nível de aninhamento a partir de 0.
    ListItem(u8),
}

/// O alinhamento horizontal de um parágrafo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    /// O começo da linha — a esquerda, no português.
    #[default]
    Start,
    /// Centralizado.
    Center,
    /// O fim da linha.
    End,
    /// Justificado.
    Justify,
}

/// Um pedaço do conteúdo de um parágrafo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Texto com as marcas dele. Tabulação chega como `\t`.
    Text {
        /// O texto.
        text: String,
        /// Negrito, itálico, sublinhado, tachado.
        marks: Marks,
    },
    /// Quebra de linha dentro do parágrafo.
    Break,
    /// A imagem de posição dada em [`Preview::images`].
    Image(usize),
}

/// As marcas de um trecho de texto, como conjunto.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Marks(u8);

impl Marks {
    /// Negrito.
    pub const BOLD: Self = Self(1);
    /// Itálico.
    pub const ITALIC: Self = Self(1 << 1);
    /// Sublinhado.
    pub const UNDERLINE: Self = Self(1 << 2);
    /// Tachado.
    pub const STRIKE: Self = Self(1 << 3);

    /// Se todas as marcas de `other` estão aqui.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Este conjunto com `mark` ligada (`on`) ou desligada.
    #[must_use]
    pub const fn with(self, mark: Self, on: bool) -> Self {
        if on {
            Self(self.0 | mark.0)
        } else {
            Self(self.0 & !mark.0)
        }
    }
}

/// Uma tabela, linha a linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// As linhas.
    pub rows: Vec<Row>,
}

/// Uma linha de tabela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// As células, sem as que outra célula mesclada cobre.
    pub cells: Vec<Cell>,
}

/// Uma célula de tabela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Quantas colunas a célula ocupa.
    pub col_span: u32,
    /// Quantas linhas a célula ocupa.
    pub row_span: u32,
    /// O conteúdo.
    pub blocks: Vec<Block>,
}

/// Uma imagem do documento, com o tipo conferido pelos bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// O formato reconhecido pela assinatura.
    pub kind: ImageType,
    /// Os bytes.
    pub bytes: Vec<u8>,
}

/// Os formatos de imagem que entram na prévia: os que o navegador desenha sem executar
/// nada. SVG fica de fora por ser documento, não imagem; EMF e WMF, por o navegador não os
/// desenhar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF.
    Gif,
    /// BMP.
    Bmp,
    /// WebP.
    Webp,
}

impl ImageType {
    /// O formato dos bytes, pela assinatura; `None` para o que não é um dos aceitos.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"\xFF\xD8\xFF") {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.starts_with(b"BM") {
            Some(Self::Bmp)
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP".as_slice()) {
            Some(Self::Webp)
        } else {
            None
        }
    }

    /// O MIME do formato.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Bmp => "image/bmp",
            Self::Webp => "image/webp",
        }
    }
}

/// O que a prévia deixou de fora, ou o que o usuário precisa saber do arquivo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Notes {
    /// O documento carrega macros. Elas nunca são executadas aqui, mas o aviso importa a
    /// quem for abrir o arquivo no Word ou no `LibreOffice`.
    pub has_macros: bool,
    /// Imagens que não entraram: vinculadas fora do arquivo, de formato que o navegador não
    /// desenha, ilegíveis ou acima do teto.
    pub skipped_images: u32,
    /// Objetos que a prévia não desenha: gráfico, forma, caixa de texto, objeto embutido,
    /// equação.
    pub skipped_objects: u32,
    /// O documento passou de um teto de texto, de blocos ou de profundidade, e a prévia
    /// mostra só o começo.
    pub truncated: bool,
}
