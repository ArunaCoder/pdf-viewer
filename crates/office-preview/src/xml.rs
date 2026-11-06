//! A leitura do XML das partes e os atalhos de travessia.
//!
//! Elemento e atributo são comparados pelo nome **local**, sem o namespace: o `.docx` tem
//! dois namespaces para o mesmo vocabulário (o transicional e o estrito, ECMA-376 parte 1
//! §A e parte 4), e a travessia só desce por elementos conhecidos, então o nome local basta
//! para distinguir.

use roxmltree::{Document, Node, ParsingOptions};

use crate::{Limits, PreviewError};

/// A árvore do XML `text`, da parte `part`.
pub fn parse<'i>(text: &'i str, part: &str, limits: &Limits) -> Result<Document<'i>, PreviewError> {
    if !nesting_within(text, limits.max_xml_depth) {
        return Err(PreviewError::TooLarge {
            what: "the element nesting of a part",
            limit: u64::try_from(limits.max_xml_depth).unwrap_or(u64::MAX),
        });
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: limits.max_xml_nodes,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(text, options).map_err(|source| match source {
        roxmltree::Error::NodesLimitReached => PreviewError::TooLarge {
            what: "the number of XML nodes in a part",
            limit: u64::from(limits.max_xml_nodes),
        },
        source => PreviewError::Xml {
            part: part.to_owned(),
            source,
        },
    })
}

/// Se o nó é o elemento de nome local `local`.
pub fn is(node: Node<'_, '_>, local: &str) -> bool {
    node.is_element() && node.tag_name().name() == local
}

/// O valor do atributo de nome local `local`.
pub fn attr<'a>(node: Node<'a, '_>, local: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == local)
        .map(|attribute| attribute.value())
}

/// O primeiro filho elemento de nome local `local`.
pub fn child<'a, 'i>(node: Node<'a, 'i>, local: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|candidate| is(*candidate, local))
}

/// Os filhos que são elementos.
pub fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

/// Os descendentes elementos de nome local `local`, em ordem de documento.
pub fn descendants<'a, 'i: 'a>(node: Node<'a, 'i>, local: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.descendants().filter(move |candidate| is(*candidate, local))
}

/// O atributo `local` do filho `element` — o `w:val` de `<w:pStyle w:val="..."/>`.
pub fn child_attr<'a>(node: Node<'a, '_>, element: &str, local: &str) -> Option<&'a str> {
    child(node, element).and_then(|found| attr(found, local))
}

/// Se o aninhamento de elementos de `text` fica dentro de `max`.
///
/// O leitor de XML desce um nível da pilha de chamadas por elemento, e um documento com
/// milhares de níveis a estouraria antes de qualquer outro teto valer. Esta varredura, sem
/// recursão, vem antes dele. Ela só precisa não subestimar: o XML que ela aceita mal formado
/// o leitor recusa em seguida.
fn nesting_within(text: &str, max: usize) -> bool {
    let mut depth = 0_usize;
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        rest = rest.get(start + 1..).unwrap_or_default();
        if let Some(after) = rest.strip_prefix("!--") {
            rest = skip_past(after, "-->");
        } else if let Some(after) = rest.strip_prefix("![CDATA[") {
            rest = skip_past(after, "]]>");
        } else if rest.starts_with(['?', '!']) {
            rest = skip_past(rest, ">");
        } else if rest.starts_with('/') {
            depth = depth.saturating_sub(1);
            rest = skip_past(rest, ">");
        } else {
            let (self_closing, after) = tag_end(rest);
            rest = after;
            if !self_closing {
                depth += 1;
                if depth > max {
                    return false;
                }
            }
        }
    }
    true
}

/// O texto depois da primeira ocorrência de `end`; vazio se ela não vem.
fn skip_past<'t>(text: &'t str, end: &str) -> &'t str {
    text.find(end)
        .and_then(|position| text.get(position + end.len()..))
        .unwrap_or_default()
}

/// O fim da marca de abertura que começa `text`: se ela se fecha sozinha (`<a/>`) e o texto
/// depois dela. O `>` dentro de valor de atributo entre aspas não fecha a marca.
fn tag_end(text: &str) -> (bool, &str) {
    let mut quote = None;
    let mut previous = ' ';
    for (position, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return (previous == '/', text.get(position + 1..).unwrap_or_default()),
            _ => {}
        }
        previous = c;
    }
    (true, "")
}

#[cfg(test)]
mod tests {
    use super::nesting_within;

    #[test]
    fn nesting_is_measured_without_recursion() {
        assert!(nesting_within("<a><b/><c x='>'><d></d></c></a>", 3));
        assert!(
            !nesting_within("<a><b/><c x='>'><d></d></c></a>", 2),
            "a, c, d: três níveis"
        );
        assert!(nesting_within(
            "<a><!-- <b><c><d> --><![CDATA[<e><f>]]><?pi <g>?></a>",
            1
        ));
        assert!(!nesting_within(&"<a>".repeat(100_000), 256));
    }
}
