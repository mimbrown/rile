//! The logical structure of a tagged PDF (SILE's `pdfstructure` package):
//! a tree of elements such as paragraphs and headings, each owning the
//! content drawn for it, so that screen readers and text extraction can
//! follow the document in reading order.

use crate::builder::{BuilderError, DocumentBuilder};

/// The standard structure types of PDF 1.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Document,
    Part,
    Art,
    Sect,
    Div,
    BlockQuote,
    Caption,
    TOC,
    TOCI,
    Index,
    P,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    L,
    LI,
    Lbl,
    LBody,
    Table,
    THead,
    TBody,
    TFoot,
    TR,
    TH,
    TD,
    Span,
    Quote,
    Note,
    Reference,
    BibEntry,
    Code,
    Link,
    Figure,
    Formula,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Document => "Document",
            Role::Part => "Part",
            Role::Art => "Art",
            Role::Sect => "Sect",
            Role::Div => "Div",
            Role::BlockQuote => "BlockQuote",
            Role::Caption => "Caption",
            Role::TOC => "TOC",
            Role::TOCI => "TOCI",
            Role::Index => "Index",
            Role::P => "P",
            Role::H1 => "H1",
            Role::H2 => "H2",
            Role::H3 => "H3",
            Role::H4 => "H4",
            Role::H5 => "H5",
            Role::H6 => "H6",
            Role::L => "L",
            Role::LI => "LI",
            Role::Lbl => "Lbl",
            Role::LBody => "LBody",
            Role::Table => "Table",
            Role::THead => "THead",
            Role::TBody => "TBody",
            Role::TFoot => "TFoot",
            Role::TR => "TR",
            Role::TH => "TH",
            Role::TD => "TD",
            Role::Span => "Span",
            Role::Quote => "Quote",
            Role::Note => "Note",
            Role::Reference => "Reference",
            Role::BibEntry => "BibEntry",
            Role::Code => "Code",
            Role::Link => "Link",
            Role::Figure => "Figure",
            Role::Formula => "Formula",
        }
    }

    /// A heading of `level`, from 1; levels past 6 are H6.
    pub fn heading(level: usize) -> Role {
        [Role::H1, Role::H2, Role::H3, Role::H4, Role::H5, Role::H6][level.clamp(1, 6) - 1]
    }

    /// Text set straight inside these goes in a paragraph of its own.
    fn holds_paragraphs(self) -> bool {
        matches!(
            self,
            Role::Document | Role::Part | Role::Art | Role::Sect | Role::Div | Role::BlockQuote | Role::LBody | Role::Note | Role::Index
        )
    }

    /// Inline elements sit in a paragraph; anything else ends one.
    fn is_inline(self) -> bool {
        matches!(
            self,
            Role::Span | Role::Quote | Role::Note | Role::Reference | Role::Code | Role::Link | Role::Figure | Role::Formula
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructKid {
    /// Content tagged with this tag.
    Content(u32),
    Element(usize),
}

#[derive(Debug, Clone)]
pub struct StructElement {
    pub role: Role,
    pub parent: Option<usize>,
    pub kids: Vec<StructKid>,
    /// A description of a figure or formula, read in place of its content.
    pub alt: Option<String>,
    pub actual_text: Option<String>,
    /// Set where the language differs from the enclosing element's.
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Auto {
    Paragraph,
    Language,
}

#[derive(Debug, Clone)]
struct Open {
    element: usize,
    tag: Option<u32>,
    auto: Option<Auto>,
}

/// The elements of a document and which of them each tag belongs to.
///
/// Content is tagged as it is added. Text added straight into an element
/// that holds paragraphs opens a paragraph, closed when the paragraph
/// ends; text in another language than its element's goes in a span
/// carrying the language.
#[derive(Debug, Clone)]
pub struct StructTree {
    pub elements: Vec<StructElement>,
    owners: Vec<usize>,
    open: Vec<Open>,
}

impl StructTree {
    pub fn new(lang: &str) -> Self {
        let root = StructElement { role: Role::Document, parent: None, kids: Vec::new(), alt: None, actual_text: None, lang: Some(lang.to_string()) };
        Self { elements: vec![root], owners: Vec::new(), open: vec![Open { element: 0, tag: None, auto: None }] }
    }

    /// The document's language.
    pub fn lang(&self) -> &str {
        self.elements[0].lang.as_deref().unwrap_or_default()
    }

    /// The element `tag`'s content belongs to.
    pub fn owner(&self, tag: u32) -> usize {
        self.owners[tag as usize]
    }

    pub fn current(&self) -> usize {
        self.open.last().expect("root").element
    }

    fn top(&self) -> &Open {
        self.open.last().expect("root")
    }

    fn effective_lang(&self, mut element: usize) -> &str {
        loop {
            let e = &self.elements[element];
            match (&e.lang, e.parent) {
                (Some(lang), _) => return lang,
                (None, Some(parent)) => element = parent,
                (None, None) => return "",
            }
        }
    }

    fn push(&mut self, role: Role, lang: &str, auto: Option<Auto>) -> usize {
        let parent = self.current();
        let lang = (self.effective_lang(parent) != lang).then(|| lang.to_string());
        let element = self.elements.len();
        self.elements.push(StructElement { role, parent: Some(parent), kids: Vec::new(), alt: None, actual_text: None, lang });
        self.elements[parent].kids.push(StructKid::Element(element));
        let top = self.open.last_mut().expect("root");
        top.tag = None;
        self.open.push(Open { element, tag: None, auto });
        element
    }

    fn pop(&mut self) {
        if self.open.len() > 1 {
            self.open.pop();
            self.open.last_mut().expect("root").tag = None;
        }
    }

    fn close_auto(&mut self, kinds: &[Auto]) {
        while self.top().auto.is_some_and(|a| kinds.contains(&a)) {
            self.pop();
        }
    }

    /// Open an element of `role` set in `lang`, inside the current one.
    pub fn begin(&mut self, role: Role, lang: &str) -> usize {
        self.close_auto(&[Auto::Language]);
        if !role.is_inline() {
            self.close_auto(&[Auto::Paragraph]);
        } else if self.elements[self.current()].role.holds_paragraphs() {
            self.push(Role::P, lang, Some(Auto::Paragraph));
        }
        self.push(role, lang, None)
    }

    /// Close the innermost element opened with `begin`.
    pub fn end(&mut self) {
        self.close_auto(&[Auto::Language, Auto::Paragraph]);
        self.pop();
    }

    /// The paragraph ended: close the one text opened, if it is current.
    pub fn end_paragraph(&mut self) {
        self.close_auto(&[Auto::Language]);
        self.close_auto(&[Auto::Paragraph]);
    }

    /// The tag for content in `lang` added now.
    pub fn tag(&mut self, lang: &str) -> u32 {
        if self.top().auto == Some(Auto::Language) && self.effective_lang(self.current()) != lang {
            self.pop();
        }
        if self.elements[self.current()].role.holds_paragraphs() {
            self.push(Role::P, lang, Some(Auto::Paragraph));
        }
        if self.effective_lang(self.current()) != lang {
            self.push(Role::Span, lang, Some(Auto::Language));
        }
        if let Some(tag) = self.top().tag {
            return tag;
        }
        let tag = self.owners.len() as u32;
        let element = self.current();
        self.owners.push(element);
        self.elements[element].kids.push(StructKid::Content(tag));
        self.open.last_mut().expect("root").tag = Some(tag);
        tag
    }
}

impl DocumentBuilder {
    /// Tag the PDF with the document's structure, for accessibility. Turn
    /// it on before adding content.
    pub fn set_tagged(&mut self, tagged: bool) -> &mut Self {
        self.structure = match (tagged, self.structure.take()) {
            (false, _) => None,
            (true, Some(tree)) => Some(tree),
            (true, None) => Some(StructTree::new(self.language())),
        };
        self
    }

    pub fn structure(&self) -> Option<&StructTree> {
        self.structure.as_ref()
    }

    /// Open an element of `role`; what is added until `end_structure`
    /// belongs to it (SILE's `\pdf:structure`).
    pub fn begin_structure(&mut self, role: Role) -> &mut Self {
        let lang = self.language().to_string();
        if self.untagged == 0
            && let Some(tree) = &mut self.structure
        {
            tree.begin(role, &lang);
        }
        self
    }

    pub fn end_structure(&mut self) -> &mut Self {
        if self.untagged == 0
            && let Some(tree) = &mut self.structure
        {
            tree.end();
        }
        self
    }

    /// `f`'s content in an element of `role`.
    pub fn with_structure<T>(&mut self, role: Role, f: impl FnOnce(&mut Self) -> Result<T, BuilderError>) -> Result<T, BuilderError> {
        self.begin_structure(role);
        let result = f(self);
        self.end_structure();
        result
    }

    /// Describe the current element, for a figure or formula.
    pub fn set_alt_text(&mut self, alt: impl Into<String>) -> &mut Self {
        if let Some(element) = self.current_element_mut() {
            element.alt = Some(alt.into());
        }
        self
    }

    /// What the current element's content reads as, when its glyphs don't
    /// say it.
    pub fn set_actual_text(&mut self, text: impl Into<String>) -> &mut Self {
        if let Some(element) = self.current_element_mut() {
            element.actual_text = Some(text.into());
        }
        self
    }

    fn current_element_mut(&mut self) -> Option<&mut StructElement> {
        let tree = self.structure.as_mut().filter(|_| self.untagged == 0)?;
        let current = tree.current();
        Some(&mut tree.elements[current])
    }

    /// A picture in its own figure, unless one is open for it.
    pub(crate) fn add_figure(&mut self, hbox: crate::node::HBox) -> &mut Self {
        if self.current_role() == Some(Role::Figure) {
            return self.add_box(hbox);
        }
        self.begin_structure(Role::Figure).add_box(hbox).end_structure()
    }

    pub(crate) fn current_role(&self) -> Option<Role> {
        let tree = self.structure.as_ref()?;
        Some(tree.elements[tree.current()].role)
    }

    /// The tag for content added now, if the document is tagged and this
    /// is not page furniture.
    pub(crate) fn current_tag(&mut self) -> Option<u32> {
        if self.untagged > 0 {
            return None;
        }
        let lang = self.language().to_string();
        Some(self.structure.as_mut()?.tag(&lang))
    }

    pub(crate) fn end_tagged_paragraph(&mut self) {
        if self.untagged == 0
            && let Some(tree) = &mut self.structure
        {
            tree.end_paragraph();
        }
    }

    /// Run `f` with what it adds left out of the structure, as page
    /// furniture such as folios and running heads.
    pub fn untagged<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.untagged += 1;
        let result = f(self);
        self.untagged -= 1;
        result
    }

    pub(crate) fn is_untagged(&self) -> bool {
        self.untagged > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kids(tree: &StructTree, element: usize) -> Vec<String> {
        tree.elements[element]
            .kids
            .iter()
            .map(|k| match k {
                StructKid::Content(t) => format!("#{t}"),
                StructKid::Element(e) => tree.elements[*e].role.name().to_string(),
            })
            .collect()
    }

    #[test]
    fn text_opens_paragraphs_and_inline_elements_stay_in_them() {
        let mut tree = StructTree::new("en");
        let first = tree.tag("en");
        tree.begin(Role::Link, "en");
        let link = tree.tag("en");
        tree.end();
        let after = tree.tag("en");
        tree.end_paragraph();
        tree.begin(Role::H1, "en");
        tree.tag("en");
        tree.end();
        assert_eq!(kids(&tree, 0), ["P", "H1"]);
        let p = tree.owner(first);
        assert_eq!(kids(&tree, p), [format!("#{first}"), "Link".into(), format!("#{after}")]);
        assert_eq!(tree.elements[tree.owner(link)].role, Role::Link);
    }

    #[test]
    fn other_languages_go_in_spans() {
        let mut tree = StructTree::new("en");
        tree.tag("en");
        let fr = tree.tag("fr");
        let back = tree.tag("en");
        let span = tree.owner(fr);
        assert_eq!(tree.elements[span].role, Role::Span);
        assert_eq!(tree.elements[span].lang.as_deref(), Some("fr"));
        assert_eq!(tree.elements[tree.owner(back)].role, Role::P);
        assert_eq!(tree.elements[tree.owner(back)].lang, None);
    }

    #[test]
    fn block_elements_end_the_paragraph() {
        let mut tree = StructTree::new("en");
        tree.tag("en");
        tree.begin(Role::L, "en");
        tree.begin(Role::LI, "en");
        tree.begin(Role::LBody, "en");
        let item = tree.tag("en");
        tree.end();
        tree.end();
        tree.end();
        assert_eq!(kids(&tree, 0), ["P", "L"]);
        let p = tree.owner(item);
        assert_eq!(tree.elements[p].role, Role::P);
        assert_eq!(tree.elements[tree.elements[p].parent.unwrap()].role, Role::LBody);
    }
}
