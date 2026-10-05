//! A PDF's first page placed as an image: its content becomes a form
//! XObject, and the objects its resources refer to are copied in.

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};
use pdf_writer::{Filter, Finish, Name, Obj, Pdf, Rect, Ref, Str};

pub(crate) struct Form {
    document: Document,
    page: ObjectId,
    page_box: [f64; 4],
}

impl Form {
    pub(crate) fn open(data: &[u8], page_box: [f64; 4]) -> Result<Self, lopdf::Error> {
        let document = Document::load_mem(data)?;
        let page = document.get_pages().into_values().next().ok_or(lopdf::Error::PageNumberNotFound(1))?;
        Ok(Self { document, page, page_box })
    }

    /// The page's resources: its own, or the nearest it inherits.
    fn resources(&self) -> Option<&lopdf::Dictionary> {
        let (own, inherited) = self.document.get_page_resources(self.page).ok()?;
        own.or_else(|| inherited.first().and_then(|id| self.document.get_dictionary(*id).ok()))
    }

    pub(crate) fn write(&self, pdf: &mut Pdf, form_ref: Ref, alloc: &mut dyn FnMut() -> Ref, compress: bool) {
        let content = self.document.get_page_content(self.page);
        let data = if compress { crate::compress_data(&content) } else { content };
        let mut copier = Copier { alloc, refs: HashMap::new(), pending: Vec::new() };

        let mut form = pdf.form_xobject(form_ref, &data);
        if compress {
            form.filter(Filter::FlateDecode);
        }
        let [left, bottom, right, top] = self.page_box.map(|v| v as f32);
        form.bbox(Rect::new(left, bottom, right, top));
        if let Some(resources) = self.resources() {
            copier.dictionary(form.insert(Name(b"Resources")), resources);
        }
        form.finish();

        while let Some((id, new)) = copier.pending.pop() {
            match self.document.get_object(id) {
                Ok(Object::Stream(stream)) => {
                    let mut out = pdf.stream(new, &stream.content);
                    for (key, value) in stream.dict.iter() {
                        if key != b"Length" {
                            copier.value(out.insert(Name(key)), value);
                        }
                    }
                }
                Ok(object) => copier.value(pdf.indirect(new), object),
                Err(_) => pdf.indirect(new).primitive(pdf_writer::Null),
            }
        }
    }
}

/// Writes objects of the placed PDF, renumbering what they refer to.
struct Copier<'a> {
    alloc: &'a mut dyn FnMut() -> Ref,
    refs: HashMap<ObjectId, Ref>,
    /// Referred to, and still to be written.
    pending: Vec<(ObjectId, Ref)>,
}

impl Copier<'_> {
    fn reference(&mut self, id: ObjectId) -> Ref {
        if let Some(new) = self.refs.get(&id) {
            return *new;
        }
        let new = (self.alloc)();
        self.refs.insert(id, new);
        self.pending.push((id, new));
        new
    }

    fn dictionary(&mut self, out: Obj<'_>, dictionary: &lopdf::Dictionary) {
        let mut out = out.dict();
        for (key, value) in dictionary.iter() {
            self.value(out.insert(Name(key)), value);
        }
    }

    fn value(&mut self, out: Obj<'_>, value: &Object) {
        match value {
            Object::Null => out.primitive(pdf_writer::Null),
            Object::Boolean(value) => out.primitive(*value),
            Object::Integer(value) => out.primitive(*value as i32),
            Object::Real(value) => out.primitive(*value),
            Object::Name(name) => out.primitive(Name(name)),
            Object::String(bytes, _) => out.primitive(Str(bytes)),
            Object::Array(items) => {
                let mut out = out.array();
                for item in items {
                    self.value(out.push(), item);
                }
            }
            Object::Dictionary(dictionary) => self.dictionary(out, dictionary),
            // A stream is always an object of its own, so none is met here.
            Object::Stream(stream) => self.dictionary(out, &stream.dict),
            Object::Reference(id) => {
                let new = self.reference(*id);
                out.primitive(new)
            }
        }
    }
}
