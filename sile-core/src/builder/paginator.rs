use std::ops::{Deref, DerefMut};

use super::*;

struct SavedTypesetter {
    settings: Settings,
    paragraph: Vec<Inline>,
    open_boxes: Vec<(Group, Vec<Inline>)>,
    current_indent: Option<f64>,
    previous_depth: Option<f64>,
    queue: Vec<Node>,
    captures: Vec<Capture>,
    lists: Vec<crate::lists::ListLevel>,
    frame: String,
}

/// A typesetter per frame, kept level with each other (SILE's `parallel`
/// package), keyed by name in the order frames are output.
struct Parallel {
    flows: BTreeMap<String, ParallelFlow>,
    active: Option<String>,
}

struct ParallelFlow {
    frame: String,
    flow: FlowState,
    /// How much of the queue is level with the other flows.
    mark: usize,
}

/// The page being filled: its frames and the frame content flows into.
struct PageState {
    page: Page,
    frame: String,
    /// What the page's frames were solved from.
    specs: Vec<FrameSpec>,
}

/// SILE's `balanced-frames` penalty: balance the columns up to here.
const BALANCE_PENALTY: i32 = -17_777;

/// Pages a `Typesetter`'s output: frames, page templates, insertions,
/// page breaking and everything else that knows about pages. The
/// typesetter's methods are available through it.
pub struct DocumentBuilder {
    ts: Typesetter,
    paper: PaperSize,
    margins: [f64; 4], // top, right, bottom, left
    header_height: f64,
    footer_height: f64,
    frame_gap: f64,
    class: Option<Box<dyn DocumentClass>>,
    /// Frames used instead of the class's (SILE's `\switch-master`).
    master: Option<PageTemplate>,
    /// Direction of frames that don't set their own.
    direction: FrameDirection,
    /// Directions set on frames while typesetting (SILE's `\thisframeRTL`).
    frame_directions: BTreeMap<String, FrameDirection>,

    page: Option<PageState>,
    pages: Vec<Page>,
    last_penalty: i32,
    insertion_classes: BTreeMap<String, InsertionClass>,
    insertions: PageInsertions,
    /// Typesetting states set aside by `push_typesetter`.
    typesetters: Vec<SavedTypesetter>,
    parallel: Option<Parallel>,
    /// Frames added to every page's template (SILE's `class:declareFrame`).
    extra_frames: Vec<FrameSpec>,

    // Running header/footer (need header_height / footer_height > 0)
    header: Option<RunningText>,
    footer: Option<RunningText>,

    metadata: Metadata,
    bookmarks: Vec<Bookmark>,
    grid_debug: Option<f64>,
    background: Option<Background>,
    end_page_hooks: Vec<PageHook>,
    best_fit_pages: bool,

    /// What the previous pass found, if there was one.
    previous_references: Option<CrossReferences>,
    consulted_references: std::cell::Cell<bool>,
    references: CrossReferences,
}

impl Deref for DocumentBuilder {
    type Target = Typesetter;

    fn deref(&self) -> &Typesetter {
        &self.ts
    }
}

impl DerefMut for DocumentBuilder {
    fn deref_mut(&mut self) -> &mut Typesetter {
        &mut self.ts
    }
}

impl Arranger for DocumentBuilder {
    fn typesetter(&mut self) -> &mut Typesetter {
        &mut self.ts
    }

    fn before_lines(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        self.sync_frame();
        Ok(())
    }

    fn after_lines(&mut self, independent: bool) -> Result<(), BuilderError> {
        if independent || !self.typesetters.is_empty() {
            return Ok(());
        }
        if self.build_page()? {
            self.init_next_frame()?;
        }
        Ok(())
    }
}

impl AsMut<DocumentBuilder> for DocumentBuilder {
    fn as_mut(&mut self) -> &mut DocumentBuilder {
        self
    }
}

impl DocumentBuilder {
    pub fn new(paper: PaperSize) -> Self {
        Self {
            ts: Typesetter::new(),
            paper,
            margins: [72.0; 4],
            header_height: 0.0,
            footer_height: 0.0,
            frame_gap: 0.0,
            class: None,
            master: None,
            direction: FrameDirection::LTR,
            frame_directions: BTreeMap::new(),
            page: None,
            pages: Vec::new(),
            last_penalty: 0,
            insertion_classes: BTreeMap::new(),
            insertions: PageInsertions::default(),
            typesetters: Vec::new(),
            parallel: None,
            extra_frames: Vec::new(),
            header: None,
            footer: None,
            metadata: Metadata::default(),
            bookmarks: Vec::new(),
            grid_debug: None,
            background: None,
            end_page_hooks: Vec::new(),
            best_fit_pages: false,
            previous_references: None,
            consulted_references: Default::default(),
            references: CrossReferences::default(),
        }
    }

    // -- Page geometry -------------------------------------------------------

    pub fn paper(&self) -> PaperSize {
        self.paper
    }

    pub fn set_page_size(&mut self, paper: PaperSize) -> &mut Self {
        self.paper = paper;
        self
    }

    pub fn set_margins(&mut self, top: f64, right: f64, bottom: f64, left: f64) -> &mut Self {
        self.margins = [top, right, bottom, left];
        self
    }

    pub fn set_header_height(&mut self, height: f64, gap: f64) -> &mut Self {
        self.header_height = height;
        self.frame_gap = gap;
        self
    }

    pub fn set_footer_height(&mut self, height: f64, gap: f64) -> &mut Self {
        self.footer_height = height;
        self.frame_gap = gap;
        self
    }

    /// The running header, set once per page into the header frame
    /// (`set_header_height` must reserve room for it).
    pub fn set_header(&mut self, header: RunningText) -> &mut Self {
        self.header = Some(header);
        self
    }

    /// The running footer (see `set_header`).
    pub fn set_footer(&mut self, footer: RunningText) -> &mut Self {
        self.footer = Some(footer);
        self
    }

    // -- Page furniture ------------------------------------------------------

    /// The values of `category` markers on the page being finished, in the
    /// order they were output (for use while it ends).
    pub fn page_info<T: std::any::Any + Clone>(&self, category: &str) -> Vec<T> {
        fn collect<T: std::any::Any + Clone>(nodes: &[Node], category: &str, out: &mut Vec<T>) {
            for node in nodes {
                match node {
                    Node::VBox(b) => collect(&b.nodes, category, out),
                    Node::HBox(b) => match &b.ink {
                        Some(Ink::Info(info)) if info.category == category => {
                            out.extend(info.value.downcast_ref::<T>().cloned());
                        }
                        _ => collect(&b.nodes, category, out),
                    },
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Some(state) = &self.page {
            for (_, nodes) in &state.page.content {
                collect(nodes, category, &mut out);
            }
        }
        out
    }

    /// The direction of every frame that doesn't set its own.
    pub fn set_direction(&mut self, direction: impl Into<FrameDirection>) -> &mut Self {
        let direction = direction.into();
        self.direction = direction;
        let template = self.page_template();
        if let Some(state) = self.page.as_mut() {
            for frame in &mut state.page.frames {
                let fixed = self.frame_directions.contains_key(&frame.id)
                    || template.frames.iter().any(|t| t.id == frame.id && t.direction.is_some());
                if !fixed {
                    frame.direction = Some(direction);
                }
            }
        }
        self.sync_frame();
        self
    }

    /// Set the writing direction of the frame being filled, here and on
    /// later pages.
    pub fn set_frame_direction(&mut self, direction: FrameDirection) -> &mut Self {
        if let Some(state) = self.page.as_mut() {
            self.frame_directions.insert(state.frame.clone(), direction);
            if let Some(frame) = state.page.frames.iter_mut().find(|f| f.id == state.frame) {
                frame.direction = Some(direction);
            }
        }
        self.sync_frame();
        self
    }

    /// Point the typesetter at the frame being filled, or before the first
    /// page, at the frame it will start in.
    fn sync_frame(&mut self) {
        let frame = match (&self.page, self.current_frame()) {
            (_, Some(frame)) => FrameContext {
                line_length: frame.line_length(),
                direction: frame.direction.unwrap_or(self.direction),
                tate: frame.tate,
            },
            (Some(_), None) => FrameContext { direction: self.direction, ..Default::default() },
            (None, None) => {
                let template = self.page_template();
                let first = template.frames.iter().find(|f| f.id == template.first_content_frame);
                let id = first.map(|f| f.id.as_str()).unwrap_or_default();
                let direction = self.frame_directions.get(id).copied().or(first.and_then(|f| f.direction)).unwrap_or(self.direction);
                FrameContext { direction, ..Default::default() }
            }
        };
        self.ts.frame = frame;
    }

    fn resolve_direction(&self, frame: &mut FrameGeometry) {
        frame.direction = self.frame_directions.get(&frame.id).copied().or(frame.direction).or(Some(self.direction));
    }

    /// Set what `content` adds as Latin text lying on its side in vertical
    /// Japanese, word by word, after a little space; elsewhere it is set as
    /// is (SILE's `\\latin-in-tate`).
    pub fn add_latin_in_tate<C, E>(ctx: &mut C, content: impl FnOnce(&mut C) -> Result<(), E>) -> Result<(), E>
    where
        C: AsMut<DocumentBuilder>,
        E: From<BuilderError>,
    {
        if ctx.as_mut().frame_direction().writing != Flow::TTB {
            return content(ctx);
        }
        let doc = ctx.as_mut();
        let saved = doc.settings.clone();
        let indent = doc.current_indent.unwrap_or(doc.settings.paragraph_indent);
        doc.set_language("und").update_font(|f| f.direction = Direction::LTR)?;
        doc.start_hbox();
        let result = content(ctx);
        let doc = ctx.as_mut();
        let inlines = doc.open_boxes.pop().map(|(_, content)| content).unwrap_or_default();
        result?;
        doc.frame_override = Some(FrameDirection::LTR);
        let nodes = doc.shape_inlines(&inlines);
        doc.frame_override = None;
        doc.settings = saved;
        let zw = doc.zenkaku_width();
        doc.add_glue(Length::new(Measurement::pt(0.5 * zw), Measurement::pt(0.25 * zw), Measurement::pt(0.25 * zw)));
        // The inner material is set as a paragraph of its own, indent included.
        doc.add_glue(Length::pt(indent));
        for mut node in nodes? {
            if node.is_glue() || node.is_kern() {
                doc.push_inline(Inline::Node(Box::new(node)));
            } else if pt_of(&node.line_contribution()) > 0.0 {
                if let Node::NNode(n) = &mut node {
                    for g in &mut n.glyphs {
                        (g.x_advance, g.x_offset, g.y_offset) = (g.width, 0.0, 0.0);
                    }
                }
                let mut hbox = natural_hbox(vec![node]);
                hbox.ink = Some(Ink::LatinInTate(zw));
                doc.add_box(hbox);
            }
        }
        Ok(())
    }

    /// Draw the character grid of the frame being filled under the page's
    /// content (SILE's `\show-hanmen`).
    pub fn show_hanmen(&mut self, hanmen: &crate::class::Hanmen) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let frame = self.current_frame().expect("current frame").clone();
        let (grid, gap) = (hanmen.gridsize, hanmen.linegap);
        let mut rules = Vec::new();
        let mut g = frame.top;
        while g < frame.bottom {
            rules.push([frame.left, g - 0.25, frame.width(), 0.5]);
            let mut l = frame.left;
            while l <= frame.right {
                rules.push([l - 0.25, g + grid - 0.25, 0.5, -grid]);
                l += grid;
            }
            g += grid;
            rules.push([frame.left, g - 0.25, frame.width(), 0.5]);
            g += gap;
        }
        let color = Color::Rgb { r: 1.0, g: 0.9, b: 0.9 };
        let page = &mut self.page.as_mut().expect("page").page;
        page.underlay.push(Underlay::Rules(Some(color), rules));
        Ok(self)
    }

    /// Set lines on a grid `spacing` apart from the top of each frame, with
    /// vertical space rounded up to fit (SILE's `\grid`).
    pub fn start_grid(&mut self, spacing: f64) -> Result<&mut Self, BuilderError> {
        self.grid = Some(Grid { spacing, cursor: 0.0 });
        self.ensure_page()?;
        self.grid_new_frame();
        Ok(self)
    }

    /// Rule the grid lines in this frame and every one after it (SILE's
    /// `\grid:debug`).
    pub fn show_grid(&mut self, spacing: f64) -> Result<&mut Self, BuilderError> {
        self.grid_debug = Some(spacing);
        self.ensure_page()?;
        self.paint_grid();
        Ok(self)
    }

    fn paint_grid(&mut self) {
        let (Some(spacing), Some(frame)) = (self.grid_debug, self.current_frame()) else { return };
        let mut rules = Vec::new();
        let mut y = spacing;
        while y < frame.height() {
            rules.push([frame.left, frame.top + y, frame.width(), 0.1]);
            y += spacing;
        }
        self.page.as_mut().expect("page").page.underlay.push(Underlay::Rules(None, rules));
    }

    /// Choose page breaks as line breaks are chosen, over several pages at
    /// once, instead of one page at a time (SILE's `pagebuilder-bestfit`).
    pub fn set_best_fit_pages(&mut self, on: bool) -> &mut Self {
        self.best_fit_pages = on;
        self
    }

    /// Lay pages out with `class`: its page template, and its hooks at
    /// every page start and end. Set before adding content.
    pub fn set_class(&mut self, class: impl DocumentClass) -> &mut Self {
        self.class = Some(Box::new(class));
        self.sync_frame();
        self
    }

    pub fn class_mut<C: DocumentClass>(&mut self) -> Option<&mut C> {
        self.class.as_deref_mut()?.as_any_mut().downcast_mut::<C>()
    }

    /// Number of the page being filled, counting from 1.
    pub fn page_number(&self) -> usize {
        self.pages.len() + 1
    }

    /// Width and height of the frame content is flowing into.
    pub fn frame_size(&mut self) -> Result<(f64, f64), BuilderError> {
        self.ensure_page()?;
        let frame = self.current_frame().expect("current frame");
        Ok((frame.width(), frame.height()))
    }

    fn ensure_page(&mut self) -> Result<(), BuilderError> {
        if self.page.is_none() {
            self.start_page()?;
        }
        Ok(())
    }

    fn page_template(&self) -> PageTemplate {
        let mut template = match (&self.master, &self.class) {
            (Some(master), _) => master.clone(),
            (None, Some(class)) => class.page_template(),
            (None, None) => self.default_template(),
        };
        template.frames.retain(|f| !self.extra_frames.iter().any(|e| e.id == f.id));
        template.frames.extend(self.extra_frames.iter().cloned());
        template
    }

    /// Split the current frame where the material so far ends, or `offset`
    /// below its top: the frame ends there and a new one, `<id>_`, takes
    /// the rest, with what follows going into it unless `offset` is given
    /// (SILE's `\breakframevertical`).
    pub fn break_frame_vertical(&mut self, offset: Option<f64>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let height = match offset {
            Some(offset) => offset,
            None => {
                self.leave_hmode(true)?;
                let queue = std::mem::take(&mut self.vertical_queue);
                let height = queue.iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum();
                let id = self.page.as_ref().expect("page").frame.clone();
                self.output(&id, queue);
                height
            }
        };
        let state = self.page.as_mut().expect("page");
        let index = state.page.frames.iter().position(|f| f.id == state.frame).ok_or_else(|| BuilderError::Layout("no current frame".into()))?;
        let mut rest = state.page.frames[index].clone();
        rest.id = format!("{}_", rest.id);
        rest.top += height;
        rest.direction = None;
        let frame = &mut state.page.frames[index];
        frame.bottom = frame.top + height;
        frame.next = Some(rest.id.clone());
        let rest_id = rest.id.clone();
        self.resolve_direction(&mut rest);
        let state = self.page.as_mut().expect("page");
        state.page.frames.push(rest);
        if offset.is_none() {
            state.frame = rest_id;
            self.previous_depth = None;
        }
        self.sync_frame();
        Ok(self)
    }

    /// Add frames to every page from this one on (SILE's
    /// `class:declareFrame`).
    pub fn declare_frames(&mut self, frames: &[FrameSpec]) -> Result<&mut Self, BuilderError> {
        self.extra_frames.retain(|e| !frames.iter().any(|f| f.id == e.id));
        self.extra_frames.extend(frames.iter().cloned());
        self.declare_page_frames(frames)
    }

    fn start_page(&mut self) -> Result<(), BuilderError> {
        let template = self.page_template();
        let mut frames = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &template.frames)
            .map_err(|e| BuilderError::Layout(e.to_string()))?;
        for frame in &mut frames {
            self.resolve_direction(frame);
        }
        if !frames.iter().any(|f| f.id == template.first_content_frame) {
            return Err(BuilderError::Layout(format!(
                "no frame {}",
                template.first_content_frame
            )));
        }
        self.insertions = PageInsertions::default();
        self.page = Some(PageState {
            page: Page::new(self.page_number(), self.paper, frames),
            frame: template.first_content_frame,
            specs: template.frames,
        });
        self.sync_frame();
        self.paint_background();
        Ok(())
    }

    /// Fill this page behind its content and, if `all_pages`, the pages
    /// after it; `None` stops filling later pages (SILE's `\background`).
    pub fn set_background(&mut self, background: Option<Background>) -> Result<&mut Self, BuilderError> {
        self.background = background;
        if self.page.is_some() {
            self.paint_background();
        } else {
            self.ensure_page()?;
        }
        Ok(self)
    }

    /// Call `hook` as each page ends, after the class's own end of page,
    /// with the page still current (SILE's `endpage` hooks).
    pub fn add_end_page_hook(&mut self, hook: impl FnMut(&mut DocumentBuilder) -> Result<(), BuilderError> + 'static) -> &mut Self {
        self.end_page_hooks.push(Box::new(hook));
        self
    }

    /// Draw `decoration` over the current page's content.
    pub fn add_overlay(&mut self, decoration: Underlay) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        if let Some(state) = self.page.as_mut() {
            state.page.overlay.push(decoration);
        }
        Ok(self)
    }

    fn paint_background(&mut self) {
        let Some(background) = &self.background else { return };
        let Some(state) = self.page.as_mut() else { return };
        let page = [0.0, 0.0, self.paper.width, self.paper.height];
        state.page.underlay.push(match &background.fill {
            BackgroundFill::Color(color) => Underlay::Rules(Some(*color), vec![page]),
            BackgroundFill::Image(image) => Underlay::Image(image.clone(), page),
        });
        if !background.all_pages {
            self.background = None;
        }
    }

    /// Declare or redeclare frames on the current page only; the next page
    /// goes back to the class's frames (SILE's `\frame`). Expressions may
    /// refer to the page's other frames.
    pub fn declare_page_frames(&mut self, frames: &[FrameSpec]) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let template = self.page_template();
        let mut specs: Vec<FrameSpec> =
            template.frames.into_iter().filter(|t| !frames.iter().any(|f| f.id == t.id)).collect();
        specs.extend(frames.iter().cloned());
        let solved = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &specs).map_err(|e| BuilderError::Layout(e.to_string()))?;
        let mut solved: Vec<FrameGeometry> = solved.into_iter().filter(|g| frames.iter().any(|f| f.id == g.id)).collect();
        for frame in &mut solved {
            self.resolve_direction(frame);
        }
        let state = self.page.as_mut().expect("page");
        state.specs.retain(|s| !frames.iter().any(|f| f.id == s.id));
        state.specs.extend(frames.iter().cloned());
        let page = &mut state.page;
        for frame in solved {
            match page.frames.iter_mut().find(|g| g.id == frame.id) {
                Some(existing) => *existing = frame,
                None => page.frames.push(frame),
            }
        }
        self.sync_frame();
        Ok(self)
    }

    /// Carry on in frame `id` of the current page, which becomes the start
    /// of this page's content frames (SILE's `\pagetemplate`).
    pub fn set_content_frame(&mut self, id: &str) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        if state.page.frame(id).is_none() {
            return Err(BuilderError::Layout(format!("no frame {id}")));
        }
        state.frame = id.to_string();
        self.sync_frame();
        Ok(self)
    }

    /// Use `template`'s frames instead of the class's from now on, starting
    /// with the current page (SILE's `\switch-master`).
    pub fn set_master(&mut self, template: PageTemplate) -> Result<&mut Self, BuilderError> {
        self.master = Some(template.clone());
        self.replace_page_frames(&template)
    }

    /// Use `template`'s frames for the rest of the current page only, after
    /// shipping what is waiting into the current frame (SILE's
    /// `\switch-master-one-page`).
    pub fn set_page_master(&mut self, template: &PageTemplate) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        self.chuck()?;
        self.replace_page_frames(template)?;
        self.leave_hmode(false)?;
        Ok(self)
    }

    /// Ship everything waiting into the current frame as it is (SILE's
    /// `chuck`).
    fn chuck(&mut self) -> Result<(), BuilderError> {
        self.leave_hmode(true)?;
        if !self.vertical_queue.is_empty() {
            let id = self.page.as_ref().expect("page").frame.clone();
            let nodes = std::mem::take(&mut self.vertical_queue);
            self.output(&id, nodes);
        }
        Ok(())
    }

    fn replace_page_frames(&mut self, template: &PageTemplate) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let frames = framespec::solve(self.paper, self.font_spec().map_or(10.0, |f| f.size), &template.frames).map_err(|e| BuilderError::Layout(e.to_string()))?;
        let state = self.page.as_mut().expect("page");
        state.page.frames.retain(|f| state.page.content.iter().any(|(id, _)| *id == f.id));
        state.page.frames.retain(|f| !frames.iter().any(|g| g.id == f.id));
        state.page.frames.extend(frames);
        state.specs.retain(|s| !template.frames.iter().any(|f| f.id == s.id));
        state.specs.extend(template.frames.iter().cloned());
        self.set_content_frame(&template.first_content_frame)
    }

    /// Split the current frame into `columns` equal columns separated by
    /// `gutter`, for the rest of this page (SILE's `\makecolumns`).
    pub fn make_columns(&mut self, columns: usize, gutter: f64) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        let Some(frame) = state.page.frame(&state.frame).cloned() else { return Ok(self) };
        if columns < 2 {
            return Ok(self);
        }
        let width = (frame.width() - gutter * (columns - 1) as f64) / columns as f64;
        let mut new_frames = Vec::new();
        let mut previous = frame.id.clone();
        for i in 1..columns {
            let left = frame.left + i as f64 * (width + gutter);
            let gutter_frame = FrameGeometry {
                id: format!("{}_gutter{i}", frame.id),
                left: left - gutter,
                right: left,
                next: None,
                ..frame.clone()
            };
            let column = FrameGeometry { id: format!("{}_col{i}", frame.id), left, right: left + width, next: None, ..frame.clone() };
            new_frames.push((previous.clone(), column.id.clone()));
            previous = column.id.clone();
            state.page.frames.push(gutter_frame);
            state.page.frames.push(column);
        }
        for (from, to) in new_frames {
            if let Some(f) = state.page.frames.iter_mut().find(|f| f.id == from) {
                f.next = Some(to);
            }
        }
        if let Some(f) = state.page.frames.iter_mut().find(|f| f.id == frame.id) {
            f.right = f.left + width;
        }
        self.sync_frame();
        Ok(self)
    }

    /// Outline frame `id` on the current page, or all its frames when
    /// `None` (SILE's `\showframe`).
    pub fn show_frame(&mut self, id: Option<&str>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let page = &mut self.page.as_mut().expect("page").page;
        let frames: Vec<FrameGeometry> = page.frames.iter().filter(|f| id.is_none_or(|id| f.id == id)).cloned().collect();
        page.outlines.extend(frames);
        Ok(self)
    }

    /// Frame `id` of the current page.
    pub fn frame(&self, id: &str) -> Option<&FrameGeometry> {
        self.page.as_ref()?.page.frame(id)
    }

    fn current_frame(&self) -> Option<&FrameGeometry> {
        let state = self.page.as_ref()?;
        state.page.frame(&state.frame)
    }

    /// Fill the current frame if the queue holds enough to (SILE's
    /// `buildPage`). Insertions met on the way are placed on this page and
    /// shrink the frames they steal from.
    fn build_page(&mut self) -> Result<bool, BuilderError> {
        if self.vertical_queue.is_empty() {
            return Ok(false);
        }
        self.ensure_page()?;
        if let Some(built) = self.build_balanced_page()? {
            return Ok(built);
        }
        self.fill_frame()
    }

    fn fill_frame(&mut self) -> Result<bool, BuilderError> {
        let frame = self.current_frame().expect("current frame");
        let id = frame.id.clone();
        let target = frame.target_length() - self.insertions.shrinkage(&id);
        let (classes, insertions) = (&self.insertion_classes, &mut self.insertions);
        let mut on_insertion = |queue: &mut Vec<Node>, i, height, target| {
            insertions.process(classes, &id, queue, i, height, target)
        };
        let ts = &mut self.ts;
        let br = if ts.grid.is_some() {
            pagebuilder::find_grid_break(&mut ts.vertical_queue, target, &mut on_insertion)
        } else if self.best_fit_pages {
            pagebuilder::find_best_fit_break(&ts.vertical_queue, target, &ts.settings.linebreak_settings)
        } else {
            pagebuilder::find_break(&mut ts.vertical_queue, target, false, &mut on_insertion)
        };
        let Some(br) = br else {
            return Ok(false);
        };
        self.last_penalty = br.trigger_penalty;
        let nodes = pagebuilder::split_page(&mut self.vertical_queue, &br);
        self.commit_shrinkage();
        let target = self.current_frame().expect("current frame").height() - self.insertions.shrinkage(&id);
        self.output(&id, pagebuilder::set_vertical_glue(nodes, target));
        Ok(true)
    }

    /// Leave the paragraph and even out this page's balanced columns up to
    /// here; what follows goes on in the frame after them (SILE's
    /// `\balancecolumns`). The end of the document, or a page break, also
    /// balances them.
    pub fn balance_columns(&mut self) -> Result<&mut Self, BuilderError> {
        self.add_vertical_penalty(BALANCE_PENALTY)
    }

    /// The current frame and the balanced frames after it, when it is
    /// balanced.
    fn balanced_columns(&self) -> Vec<FrameGeometry> {
        let mut columns: Vec<FrameGeometry> = Vec::new();
        let Some(state) = self.page.as_ref() else { return columns };
        let mut frame = state.page.frame(&state.frame);
        while let Some(f) = frame.filter(|f| f.balanced && !f.direction.is_some_and(FrameDirection::is_vertical) && !columns.iter().any(|c| c.id == f.id)) {
            columns.push(f.clone());
            frame = f.next.as_deref().and_then(|next| state.page.frame(next));
        }
        columns
    }

    /// Fill a run of balanced columns (SILE's `balanced-frames`), or `None`
    /// to fill the current frame as usual. Material waits until it is to be
    /// balanced, overflows the columns or holds a forced break; then the
    /// columns are cut to the least height that holds it, moving frames
    /// placed relative to them.
    fn build_balanced_page(&mut self) -> Result<Option<bool>, BuilderError> {
        let columns = self.balanced_columns();
        if columns.len() < 2 {
            return Ok(None);
        }
        let is_penalty = |n: &Node, max| matches!(n, Node::Penalty(p) if p.penalty <= max);
        let Some(end) = self.vertical_queue.iter().position(|n| is_penalty(n, BALANCE_PENALTY)) else {
            let room: f64 = columns.iter().map(FrameGeometry::height).sum();
            let natural: f64 = self.vertical_queue.iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum();
            let forced = self.vertical_queue.iter().any(|n| is_penalty(n, -10_000));
            return Ok((natural <= room && !forced).then_some(false));
        };
        let Some(height) = self.balanced_height(&self.vertical_queue[..end], columns.len(), columns.iter().map(FrameGeometry::height).fold(f64::INFINITY, f64::min)) else {
            return Ok(None);
        };
        self.set_column_height(&columns, height);
        let rest = self.vertical_queue.split_off(end);
        let Some(Node::Penalty(penalty)) = rest.first() else { unreachable!() };
        let penalty = penalty.penalty;
        self.vertical_queue.push(Node::penalty(-10_000));
        for column in &columns {
            self.page.as_mut().expect("page").frame = column.id.clone();
            self.sync_frame();
            if self.vertical_queue.iter().all(|n| n.is_discardable() || n.is_vglue()) {
                break;
            }
            self.fill_frame()?;
        }
        if self.vertical_queue.last().is_some_and(|n| is_penalty(n, -10_000)) {
            self.vertical_queue.pop();
        }
        self.vertical_queue.extend(rest.into_iter().skip(1));
        self.last_penalty = penalty;
        Ok(Some(true))
    }

    /// Cut balanced columns to `height`, re-solving the frames placed
    /// relative to them.
    fn set_column_height(&mut self, columns: &[FrameGeometry], height: f64) {
        let before_specs = self.page.as_ref().expect("page").specs.clone();
        let mut specs = before_specs.clone();
        for spec in &mut specs {
            if let Some(column) = columns.iter().find(|c| c.id == spec.id) {
                spec.top = Some(format!("{}pt", column.top));
                spec.height = Some(format!("{height}pt"));
                spec.bottom = None;
            }
        }
        let em = self.font_spec().map_or(10.0, |f| f.size);
        let mut moved: Vec<FrameGeometry> = match (framespec::solve(self.paper, em, &before_specs), framespec::solve(self.paper, em, &specs)) {
            (Ok(before), Ok(after)) => before.into_iter().zip(after).filter(|(b, a)| b != a).map(|(_, a)| a).collect(),
            _ => Vec::new(),
        };
        for frame in &mut moved {
            self.resolve_direction(frame);
        }
        let page = &mut self.page.as_mut().expect("page").page;
        for frame in moved {
            if let Some(existing) = page.frames.iter_mut().find(|f| f.id == frame.id) {
                *existing = frame;
            }
        }
        for column in columns {
            if let Some(frame) = page.frames.iter_mut().find(|f| f.id == column.id) {
                frame.bottom = frame.top + height;
            }
        }
        self.sync_frame();
    }

    /// Take the room promised to this page's insertions from the frames
    /// they steal from, at the bottom.
    fn commit_shrinkage(&mut self) {
        let Some(state) = self.page.as_mut() else { return };
        for (class, _) in &self.insertions.boxes {
            let Some(class) = self.insertion_classes.get(class) else { continue };
            for (frame, _) in &class.steal_from {
                let shrinkage = self.insertions.shrinkage.insert(frame.clone(), 0.0).unwrap_or(0.0);
                if let Some(frame) = state.page.frames.iter_mut().find(|f| &f.id == frame) {
                    frame.bottom -= shrinkage;
                }
            }
        }
        self.sync_frame();
    }

    /// Grow each insertion frame upwards by what was placed in it and set
    /// the material there.
    fn output_insertions(&mut self) {
        let boxes = std::mem::take(&mut self.insertions.boxes);
        let Some(state) = self.page.as_mut() else { return };
        for (class, stack) in &boxes {
            let Some(class) = self.insertion_classes.get(class) else { continue };
            if let Some(frame) = state.page.frames.iter_mut().find(|f| f.id == class.insert_into) {
                frame.top -= stack.height + stack.depth;
            }
        }
        for (class, stack) in boxes {
            let Some(class) = self.insertion_classes.get(&class) else { continue };
            state.page.add_frame_content(class.insert_into.clone(), stack.nodes);
        }
        self.sync_frame();
    }

    /// Declare a kind of insertion (footnotes, say).
    pub fn set_insertion_class(&mut self, name: impl Into<String>, class: InsertionClass) -> &mut Self {
        self.insertion_classes.insert(name.into(), class);
        self
    }

    pub fn insertion_class_mut(&mut self, name: &str) -> Option<&mut InsertionClass> {
        self.insertion_classes.get_mut(name)
    }

    /// Send vertical material to the frame of insertion class `class`, from
    /// the current point of the paragraph (SILE's `class:insert`).
    pub fn insert(&mut self, class: &str, nodes: Vec<Node>) -> &mut Self {
        let penalty = self.insertion_classes.get(class).map_or(-3000, |c| c.penalty);
        let stack = Stack::of(nodes);
        let insertion = Node::Insertion(node::Insertion {
            class: class.to_string(),
            nodes: stack.nodes,
            content_height: stack.height,
            content_depth: stack.depth,
            seen: false,
        });
        let migrating = node::Migrating { material: vec![Node::penalty(penalty), insertion], ..Default::default() };
        self.push_inline(Inline::Node(Box::new(Node::Migrating(migrating))));
        self
    }

    fn output(&mut self, frame: &str, nodes: Vec<Node>) {
        if let Some(state) = self.page.as_mut() {
            state.page.add_frame_content(frame, nodes);
        }
    }

    /// Move on to the next frame, or end the page and start a new one.
    fn init_next_frame(&mut self) -> Result<(), BuilderError> {
        if self.vertical_queue.is_empty() {
            self.previous_depth = None;
        }
        let old_width = self.current_frame().map(FrameGeometry::line_length);
        let next = self.current_frame().and_then(|f| f.next.clone());
        match next {
            Some(next) if self.last_penalty > SUPER_EJECT => {
                self.page.as_mut().expect("page").frame = next;
                self.sync_frame();
            }
            _ => {
                self.end_page()?;
                self.new_page()?;
            }
        }
        let new_width = self.current_frame().map(FrameGeometry::line_length);
        if !self.vertical_queue.is_empty() && old_width.zip(new_width).is_some_and(|(a, b)| (a - b).abs() > 1e-6) {
            self.push_back()?;
            if self.typesetters.is_empty() && self.build_page()? {
                self.init_next_frame()?;
            }
        } else if let Some(first) = self.vertical_queue.first() {
            let lead = match self.settings.line_spacing {
                Some(spacing) => {
                    let em = self.font_spec().map_or(10.0, |f| f.size);
                    let min = pt_of(&resolve_em(spacing.minimum_first_line, em));
                    (min > 0.0).then(|| Node::vkern(Length::pt(min - pt_of(&first.height()))))
                }
                None => Some(Node::vglue(Length::zero())),
            };
            if let Some(lead) = lead {
                self.vertical_queue.insert(0, lead);
            }
        }
        self.grid_new_frame();
        self.paint_grid();
        Ok(())
    }

    fn end_page_untagged(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        self.output_insertions();
        self.collect_references();
        if let Some(mut class) = self.class.take() {
            let result = class.end_page(self);
            self.class = Some(class);
            result?;
        }
        let mut hooks = std::mem::take(&mut self.end_page_hooks);
        let result = hooks.iter_mut().try_for_each(|hook| hook(self));
        hooks.append(&mut self.end_page_hooks);
        self.end_page_hooks = hooks;
        result?;
        if let Some(state) = self.page.take() {
            self.pages.push(state.page);
        }
        self.sync_frame();
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), BuilderError> {
        self.untagged(Self::end_page_untagged)
    }

    fn deactivate(&mut self, parallel: &mut Parallel) {
        let flow = self.take_flow();
        if let Some(p) = parallel.active.as_ref().and_then(|a| parallel.flows.get_mut(a)) {
            p.flow = flow;
        }
    }

    fn new_page(&mut self) -> Result<(), BuilderError> {
        if let Some(mut class) = self.class.take() {
            let result = self.untagged(|doc| class.new_page(doc));
            self.class = Some(class);
            result?;
        }
        self.start_page()
    }

    /// Fill the last page and end it (SILE's `class:finish`).
    fn finish(&mut self) -> Result<(), BuilderError> {
        self.ensure_page()?;
        if let Some(mut parallel) = self.parallel.take() {
            self.deactivate(&mut parallel);
            self.sync_flows(&mut parallel)?;
            self.output_parallel_page(&mut parallel)?;
            self.pop_typesetter()?;
            self.vertical_queue.clear();
            return Ok(());
        }
        self.new_paragraph()?;
        self.add_vfill()?;
        while !self.is_queue_empty() {
            self.supereject()?;
            self.leave_hmode(true)?;
            self.build_page()?;
            if !self.is_queue_empty() {
                self.init_next_frame()?;
            }
        }
        self.end_page()
    }

    /// Typeset into several frames side by side, each named flow going to
    /// its frame (`(name, frame)`), selected with `select_parallel` and
    /// kept level with `sync_parallel` (SILE's `parallel` package). As in
    /// SILE, what is added before a flow is selected goes nowhere, and so
    /// does material not yet on a page when this is called.
    pub fn begin_parallel(&mut self, flows: &[(&str, &str)]) -> Result<&mut Self, BuilderError> {
        self.push_typesetter(None)?;
        let flows = flows
            .iter()
            .map(|(name, frame)| (name.to_string(), ParallelFlow { frame: frame.to_string(), flow: FlowState::default(), mark: 0 }))
            .collect();
        self.parallel = Some(Parallel { flows, active: None });
        Ok(self)
    }

    pub fn select_parallel(&mut self, name: &str) -> Result<&mut Self, BuilderError> {
        let mut parallel = self.parallel.take().ok_or_else(|| BuilderError::Layout("no parallel flows".into()))?;
        if !parallel.flows.contains_key(name) {
            self.parallel = Some(parallel);
            return Err(BuilderError::Layout(format!("no parallel flow {name}")));
        }
        self.deactivate(&mut parallel);
        parallel.active = Some(name.to_string());
        self.activate(&mut parallel);
        self.parallel = Some(parallel);
        Ok(self)
    }

    /// Pad the flows with space so that what comes next in each starts at
    /// the same height, or end the page if any has filled it (SILE's
    /// `\sync`).
    pub fn sync_parallel(&mut self) -> Result<&mut Self, BuilderError> {
        let mut parallel = self.parallel.take().ok_or_else(|| BuilderError::Layout("no parallel flows".into()))?;
        self.deactivate(&mut parallel);
        let result = self.sync_flows(&mut parallel);
        self.activate(&mut parallel);
        self.parallel = Some(parallel);
        result.map(|_| self)
    }

    fn sync_flows(&mut self, parallel: &mut Parallel) -> Result<(), BuilderError> {
        let mut any_break = false;
        for p in parallel.flows.values_mut() {
            self.enter_flow(p);
            let result = self.leave_hmode(true);
            if result.is_ok() {
                let target = self.current_frame().map_or(0.0, FrameGeometry::target_length);
                let mut lines = self.vertical_queue.clone();
                any_break |= pagebuilder::find_break(&mut lines, target, false, &mut pagebuilder::no_insertions).is_some();
            }
            p.flow = self.take_flow();
            result?;
        }
        if any_break {
            return self.parallel_page_break(parallel);
        }
        let new_material = |p: &ParallelFlow| p.flow.queue[p.mark..].iter().map(|n| pt_of(&n.height()) + pt_of(&n.depth())).sum::<f64>();
        let tallest = parallel.flows.values().map(new_material).fold(0.0, f64::max);
        for p in parallel.flows.values_mut() {
            let glue = tallest - new_material(p);
            if glue > 0.0 {
                p.flow.queue.push(Node::vglue(Length::pt(glue)));
            }
            p.mark = p.flow.queue.len();
        }
        Ok(())
    }

    /// Output each flow's levelled material and start a new page, levelling
    /// what is left (SILE's `parallelPagebreak`).
    fn parallel_page_break(&mut self, parallel: &mut Parallel) -> Result<(), BuilderError> {
        self.output_parallel_page(parallel)?;
        self.new_page()?;
        self.sync_flows(parallel)
    }

    fn output_parallel_page(&mut self, parallel: &mut Parallel) -> Result<(), BuilderError> {
        for p in parallel.flows.values_mut() {
            self.enter_flow(p);
            let result = if !self.vertical_queue.is_empty() && p.mark == 0 {
                self.build_page().map(|_| ())
            } else {
                let queue = &mut self.ts.vertical_queue;
                let lines = queue.drain(..p.mark.min(queue.len())).collect();
                self.output(&p.frame, lines);
                Ok(())
            };
            p.flow = self.take_flow();
            result?;
        }
        self.end_page()?;
        for p in parallel.flows.values_mut() {
            p.mark = 0;
        }
        Ok(())
    }

    fn enter_flow(&mut self, p: &mut ParallelFlow) {
        let flow = std::mem::take(&mut p.flow);
        self.put_flow(flow);
        if let Some(state) = self.page.as_mut() {
            state.frame = p.frame.clone();
        }
        self.sync_frame();
    }

    fn activate(&mut self, parallel: &mut Parallel) {
        if let Some(p) = parallel.active.as_ref().and_then(|a| parallel.flows.get_mut(a)) {
            self.enter_flow(p);
        }
    }

    /// Set the current paragraph and vertical list aside and start afresh,
    /// with lines as wide as `frame` (the current frame when `None`), until
    /// `pop_typesetter` (SILE's `typesetter:pushState`).
    pub fn push_typesetter(&mut self, frame: Option<&str>) -> Result<&mut Self, BuilderError> {
        self.ensure_page()?;
        let state = self.page.as_mut().expect("page");
        let flow = match frame {
            Some(frame) => std::mem::replace(&mut state.frame, frame.to_string()),
            None => state.frame.clone(),
        };
        let ts = &mut self.ts;
        self.typesetters.push(SavedTypesetter {
            settings: ts.settings.clone(),
            paragraph: std::mem::take(&mut ts.paragraph),
            open_boxes: std::mem::take(&mut ts.open_boxes),
            current_indent: ts.current_indent.take(),
            previous_depth: ts.previous_depth.take(),
            queue: std::mem::take(&mut ts.vertical_queue),
            captures: std::mem::take(&mut ts.captures),
            lists: std::mem::take(&mut ts.lists.levels),
            frame: flow,
        });
        self.sync_frame();
        Ok(self)
    }

    /// End the paragraph begun since `push_typesetter`, restore what it set
    /// aside, and return the vertical material set meanwhile.
    pub fn pop_typesetter(&mut self) -> Result<Vec<Node>, BuilderError> {
        let result = self.leave_hmode(true);
        let Some(saved) = self.typesetters.pop() else {
            return Ok(Vec::new());
        };
        let nodes = std::mem::replace(&mut self.vertical_queue, saved.queue);
        if let Some(state) = self.page.as_mut() {
            state.frame = saved.frame;
        }
        self.sync_frame();
        self.settings = saved.settings;
        self.paragraph = saved.paragraph;
        self.open_boxes = saved.open_boxes;
        self.current_indent = saved.current_indent;
        self.previous_depth = saved.previous_depth;
        self.captures = saved.captures;
        self.lists.levels = saved.lists;
        result?;
        Ok(nodes)
    }

    /// Typeset whatever `f` adds straight into `frame` on the current page,
    /// apart from the main flow, with settings restored afterwards (SILE's
    /// `typesetNaturally`).
    pub fn typeset_into(
        &mut self,
        frame: &str,
        f: impl FnOnce(&mut Self) -> Result<(), BuilderError>,
    ) -> Result<(), BuilderError> {
        self.push_typesetter(Some(frame))?;
        let result = f(self);
        let nodes = self.pop_typesetter()?;
        result?;
        let top = nodes
            .iter()
            .position(|n| !n.is_discardable() && !n.is_explicit())
            .unwrap_or(nodes.len());
        self.output(frame, nodes.into_iter().skip(top).collect());
        Ok(())
    }

    // -- Bookmarks -----------------------------------------------------------

    /// Bookmark this point in the document outline.
    pub fn add_bookmark(&mut self, title: impl Into<String>, level: u32) -> &mut Self {
        let dest = self.new_destination();
        self.add_bookmark_at(title, level, dest)
    }

    /// Bookmark the destination `dest` in the document outline.
    pub fn add_bookmark_at(&mut self, title: impl Into<String>, level: u32, dest: impl Into<String>) -> &mut Self {
        self.bookmarks.push(Bookmark { title: title.into(), level, dest: dest.into() });
        self
    }

    /// Give this pass what the previous one found, for `references`.
    pub fn set_references(&mut self, previous: Option<CrossReferences>) -> &mut Self {
        self.previous_references = previous;
        self
    }

    /// What the previous pass found, or `None` on the first pass.
    pub fn references(&self) -> Option<&CrossReferences> {
        self.consulted_references.set(true);
        self.previous_references.as_ref()
    }

    /// Enter a heading in the table of contents and the document outline,
    /// as on the page this point ends up on (SILE's `\tocentry`).
    pub fn add_toc_entry(&mut self, level: usize, number: Option<String>, label: impl Into<String>) -> &mut Self {
        let label = label.into();
        let dest = self.new_destination();
        self.add_bookmark_at(label.clone(), level as u32, dest.clone());
        let entry = TocEntry { label, level, number, page: String::new(), dest: Some(dest) };
        self.add_info(references::TOC, entry);
        self
    }

    /// Note the references on the page being finished, numbered as the
    /// class numbers it.
    fn collect_references(&mut self) {
        let number = self.class.as_ref().and_then(|c| c.folio()).unwrap_or_else(|| PageNumber::arabic(self.pages.len() as i64 + 1));
        let page = number.to_string();
        for mut entry in self.page_info::<TocEntry>(references::TOC) {
            entry.page = page.clone();
            self.references.toc.push(entry);
        }
        for (name, mut label) in self.page_info::<(String, Label)>(references::LABELS) {
            label.page = page.clone();
            self.references.labels.entry(name).or_insert(label);
        }
        for mark in self.page_info::<IndexMark>(references::INDEX) {
            let pages = self.references.index.entry(mark.index).or_default().entry(mark.label).or_default();
            if pages.last().is_none_or(|p| p.page != number) {
                pages.push(IndexPage { page: number.clone(), link: Some(mark.link) });
            }
        }
    }

    // -- Metadata ------------------------------------------------------------

    /// Print pages centred on sheets of `sheet`, when given (SILE's
    /// `sheetsize` class option).
    pub fn set_sheet_size(&mut self, sheet: Option<PaperSize>) -> &mut Self {
        self.metadata.sheet = sheet;
        self
    }

    pub fn set_title(&mut self, title: impl Into<String>) -> &mut Self {
        self.metadata.title = Some(title.into());
        self
    }

    pub fn set_author(&mut self, author: impl Into<String>) -> &mut Self {
        self.metadata.author = Some(author.into());
        self
    }

    pub fn set_subject(&mut self, subject: impl Into<String>) -> &mut Self {
        self.metadata.subject = Some(subject.into());
        self
    }

    /// Set a document info entry by its PDF key (SILE's `\pdf:metadata`).
    /// Dates must be PDF dates; `Trapped` is not text and can't be set.
    pub fn set_pdf_metadata(&mut self, key: &str, value: &str) -> Result<&mut Self, BuilderError> {
        let invalid = |what: String| Err(BuilderError::InvalidMetadata(what));
        match key {
            "Title" => self.metadata.title = Some(value.into()),
            "Author" => self.metadata.author = Some(value.into()),
            "Subject" => self.metadata.subject = Some(value.into()),
            "Trapped" => return invalid("Trapped can't be set as text".into()),
            "CreationDate" | "ModDate" if !is_pdf_date(value) => return invalid(format!("{key} {value:?} is not a PDF date")),
            _ => self.metadata.info.push((key.into(), value.into())),
        }
        Ok(self)
    }

    // -- Render --------------------------------------------------------------

    /// Lay the document out and describe it in SILE's debug outputter format,
    /// for comparing layout against SILE's regression test expectations.
    pub fn render_debug(self) -> Result<String, BuilderError> {
        Ok(self.lay_out()?.render_debug())
    }

    /// Lay the document out and hand back its pages.
    pub fn into_pages(self) -> Result<Vec<Page>, BuilderError> {
        Ok(self.lay_out()?.pages)
    }

    /// Finish the document and lay out its last pages.
    pub fn lay_out(mut self) -> Result<Layout, BuilderError> {
        self.finish()?;
        let mut pages = std::mem::take(&mut self.pages);

        // Running header/footer: typeset per page (page numbers differ)
        let total = pages.len();
        for (name, running) in [("header", self.header.clone()), ("footer", self.footer.clone())] {
            let Some(running) = running else { continue };
            for page in pages.iter_mut() {
                let Some(hsize) = page.frame(name).map(FrameGeometry::line_length) else { continue };
                let line = running.resolved(page.number, total, &self.settings.language);
                let skips = LineSkips::default().aligned(running.align);
                let nodes = self.typeset_inlines(&line, hsize, running.direction, skips, &mut None)?;
                page.add_frame_content(name, nodes);
            }
        }

        Ok(Layout {
            pages,
            references: self.references,
            consulted_references: self.consulted_references.get(),
            paper: self.paper,
            fonts: self.ts.fonts,
            bookmarks: self.bookmarks,
            metadata: self.metadata,
            structure: self.ts.structure,
        })
    }

    // -- Page templates ------------------------------------------------------

    /// The page template from the margins: a content frame, plus a header
    /// frame inside the top margin area and/or a footer frame inside the
    /// bottom one when heights were reserved (each pushes the content frame
    /// inward by its height plus the gap).
    fn default_template(&self) -> PageTemplate {
        let [top, right, bottom, left] = self.margins;
        let pt = |v: f64| format!("{v}pt");
        let (left, right) = (pt(left), pt(self.paper.width - right));
        let mut frames = Vec::new();
        let mut body_top = top;
        let mut body_bottom = self.paper.height - bottom;
        if self.header_height > 0.0 {
            frames.push(
                FrameSpec::new("header")
                    .left(&left)
                    .right(&right)
                    .top(pt(top))
                    .height(pt(self.header_height)),
            );
            body_top += self.header_height + self.frame_gap;
        }
        if self.footer_height > 0.0 {
            frames.push(
                FrameSpec::new("footer")
                    .left(&left)
                    .right(&right)
                    .bottom(pt(body_bottom))
                    .height(pt(self.footer_height)),
            );
            body_bottom -= self.footer_height + self.frame_gap;
        }
        frames.push(
            FrameSpec::new("content")
                .left(left)
                .right(right)
                .top(pt(body_top))
                .bottom(pt(body_bottom)),
        );
        PageTemplate { frames, first_content_frame: "content".to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_builder() {
        let doc = DocumentBuilder::new(PaperSize::A4);
        assert!((doc.paper.width - 595.276).abs() < 0.01);
    }

    #[test]
    fn set_margins() {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.set_margins(50.0, 60.0, 70.0, 80.0);
        assert_eq!(doc.margins, [50.0, 60.0, 70.0, 80.0]);
    }

    #[test]
    fn set_page_size() {
        let mut doc = DocumentBuilder::new(PaperSize::A4);
        doc.set_page_size(PaperSize::LETTER);
        assert!((doc.paper.width - 612.0).abs() < 0.01);
    }
}
