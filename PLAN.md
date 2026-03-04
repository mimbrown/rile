# Refactoring Plan: Break Up the DocumentBuilder God Object

## Problem

`DocumentBuilder` in `sile-core/src/builder.rs` has 30+ fields and 33+ methods spanning 7 unrelated concerns: page geometry, font management, hyphenation, style state, paragraph buffering, layout settings, and PDF output. This makes every subsystem untestable in isolation and creates hidden coupling between unrelated state.

## Approach

Extract focused types that own their own state. `DocumentBuilder` becomes a thin coordinator that delegates to these types rather than doing everything itself. The public API changes minimally — the same builder-pattern calls work, they just route to the appropriate sub-object.

## Step 1: Extract `FontRegistry`

Pull font loading and lookup into its own type.

```rust
// New type in builder.rs (not a new module — fonts are builder-internal)
pub struct FontRegistry {
    db: FontDatabase,
    fonts: HashMap<String, RegisteredFont>,
    shaper: Box<dyn Shaper>,
}
```

Moves these methods off DocumentBuilder:
- `load_system_fonts()` → `FontRegistry::load_system_fonts()`
- `load_font_file()` → `FontRegistry::load_font_file()`
- `load_font_data()` → `FontRegistry::load_font_data()`
- `load_font_by_family()` → `FontRegistry::load_font_by_family()`

DocumentBuilder gets a `fonts: FontRegistry` field and delegates.

## Step 2: Extract `LayoutConfig`

Group page geometry into a config struct.

```rust
pub struct LayoutConfig {
    pub paper: PaperSize,
    pub margins: [f64; 4],
    pub header_height: f64,
    pub footer_height: f64,
    pub frame_gap: f64,
}
```

`build_layout()` becomes `LayoutConfig::build_layout()`. The setters (`set_page_size`, `set_margins`, `set_header_height`, `set_footer_height`) delegate to this struct.

## Step 3: Extract `OutputConfig`

Group PDF metadata into a config struct.

```rust
pub struct OutputConfig {
    pub pdf_config: PdfConfig,
    pub bookmarks: Vec<Bookmark>,
}
```

`set_title`, `set_author`, `set_subject`, `set_compress`, and `add_bookmark` delegate to this struct.

## Step 4: Extract `ParagraphBuilder`

The largest extraction — pull paragraph state and typesetting logic into its own type.

```rust
pub struct ParagraphBuilder {
    runs: Vec<TextRun>,
    indent: f64,
    skip: f64,
    leading: f64,
    space_settings: SpaceSettings,
    first_paragraph: bool,
    alignment: TextAlign,
    direction: Direction,
    linebreak_settings: LinebreakSettings,
    language: String,
    hyphenation: HyphenationDictionary,
}
```

Moves these methods:
- `add_text()` → `ParagraphBuilder::add_text()`
- `typeset_paragraph()` → `ParagraphBuilder::typeset()`
- `build_nnode()` → `ParagraphBuilder::build_nnode()`
- `build_lines()` → `ParagraphBuilder::build_lines()`

`ParagraphBuilder::typeset()` takes `&FontRegistry` and `hsize: f64` as parameters — explicit dependency injection instead of reaching into DocumentBuilder's fields.

## Step 5: Slim down `DocumentBuilder`

After extraction, DocumentBuilder becomes:

```rust
pub struct DocumentBuilder {
    layout: LayoutConfig,
    fonts: FontRegistry,
    paragraph: ParagraphBuilder,
    output: OutputConfig,
    // Remaining state
    current_font: Option<String>,
    current_color: Option<Color>,
    vertical_queue: Vec<Node>,
    page_break_settings: PageBreakSettings,
    page_count: usize,
}
```

The public API stays the same — `doc.set_margins(...)` still works, it just routes to `self.layout.margins = ...`. The `render()` method orchestrates the sub-objects.

## Step 6: Update tests

All existing tests should pass with minimal changes since the public API is preserved. Internal test helpers (like `builder_with_font()`) may need minor adjustments to access fields through the sub-structs.

## What stays on DocumentBuilder

- `current_font` / `current_color` — these are document-level cursor state
- `vertical_queue` — the accumulated page content
- `page_break_settings` / `page_count` — render-time concerns
- `render()` — the orchestration method
- `new_paragraph()` — coordinates between ParagraphBuilder and vertical_queue
- `add_vskip()`, `add_page_break()`, `add_rule()` — thin wrappers over vertical_queue

## Ordering

Steps 1-4 can each be done and verified independently. Step 5 is the final assembly. Each step should compile and pass tests before moving on.
