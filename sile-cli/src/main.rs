use sile_core::builder::{DocumentBuilder, TextAlign};
use sile_core::color::Color;
use sile_core::font::{Direction, FontSpec, FontWeight};
use sile_core::frame::PaperSize;

fn main() {
    let output_path = std::env::args().nth(1).unwrap_or_else(|| "output.pdf".into());

    // Discover a usable system font
    let (font_data, family) = load_system_font()
        .expect("no system fonts found — install at least one TrueType/OpenType font");

    println!("Using font: {family}");

    // --- Build the document ---

    let mut doc = DocumentBuilder::new(PaperSize::A4);

    // PDF metadata
    doc.set_title("A Scandal in Bohemia")
        .set_author("sile-rust")
        .set_compress(true);

    // Page margins (1 inch = 72pt)
    doc.set_margins(72.0, 72.0, 72.0, 72.0);

    // Register two fonts: heading (18pt bold) and body (11pt normal)
    let heading_spec = FontSpec {
        family: Some(family.clone()),
        size: 18.0,
        weight: FontWeight::BOLD,
        ..Default::default()
    };
    doc.load_font_data("heading", font_data.clone(), heading_spec)
        .expect("failed to load heading font");

    let body_spec = FontSpec {
        family: Some(family),
        size: 11.0,
        ..Default::default()
    };
    doc.load_font_data("body", font_data, body_spec)
        .expect("failed to load body font");

    // Typographic settings
    doc.set_language("en");
    doc.set_paragraph_indent(20.0);
    doc.set_paragraph_skip(6.0);
    doc.set_leading(3.0);

    // --- Title ---

    doc.set_font("heading");
    doc.set_paragraph_indent(0.0);
    doc.add_text("A Scandal in Bohemia");
    doc.new_paragraph().expect("title paragraph");

    doc.add_vskip(12.0);

    // --- Body text ---

    doc.set_font("body");
    doc.set_paragraph_indent(20.0);

    doc.add_text(
        "To Sherlock Holmes she is always the woman. I have seldom heard him mention \
         her under any other name. In his eyes she eclipses and predominates the whole \
         of her sex. It was not that he felt any emotion akin to love for Irene Adler. \
         All emotions, and that one particularly, were abhorrent to his cold, precise \
         but admirably balanced mind. He was, I take it, the most perfect reasoning and \
         observing machine that the world has seen, but as a lover he would have placed \
         himself in a false position.",
    );
    doc.new_paragraph().expect("paragraph 1");

    doc.add_text(
        "He never spoke of the softer passions, save with a gibe and a sneer. They \
         were admirable things for the observer \u{2014} excellent for drawing the veil \
         from men\u{2019}s motives and actions. But for the trained reasoner to admit \
         such intrusions into his own delicate and finely adjusted temperament was to \
         introduce a distracting factor which might throw a doubt upon all his mental \
         results. Grit in a sensitive instrument, or a crack in one of his own \
         high-power lenses, would not be more disturbing than a strong emotion in a \
         nature such as his.",
    );
    doc.new_paragraph().expect("paragraph 2");

    doc.set_color(Color::Rgb {
        r: 0.6,
        g: 0.0,
        b: 0.0,
    });
    doc.add_text(
        "And yet there was but one woman to him, and that woman was the late Irene \
         Adler, of dubious and questionable memory.",
    );
    doc.clear_color();
    doc.new_paragraph().expect("paragraph 3");

    doc.add_text(
        "I had seen little of Holmes lately. My marriage had drifted us away from each \
         other. My own complete happiness, and the home-centred interests which rise up \
         around the man who first finds himself master of his own establishment, were \
         sufficient to absorb all my attention, while Holmes, who loathed every form of \
         society with his whole Bohemian soul, remained in our lodgings in Baker Street, \
         buried among his old books, and alternating from week to week between cocaine \
         and ambition, the drowsiness of the drug, and the fierce energy of his own \
         keen nature.",
    );
    doc.new_paragraph().expect("paragraph 4");

    doc.add_text(
        "He was still, as ever, deeply attracted by the study of crime, and occupied \
         his immense faculties and extraordinary powers of observation in following out \
         those clues, and clearing up those mysteries which had been abandoned as \
         hopeless by the official police. From time to time I heard some vague account \
         of his doings: of his summons to Odessa in the case of the Trepoff murder, of \
         his clearing up of the singular tragedy of the Atkinson brothers at Trincomalee, \
         and finally of the mission which he had accomplished so delicately and \
         successfully for the reigning family of Holland.",
    );
    doc.new_paragraph().expect("paragraph 5");

    // --- Alignment examples ---

    doc.add_vskip(18.0);

    let demo_text = "The art of typesetting lies in the invisible details — the \
         spacing between words, the rhythm of line breaks, and the way a paragraph \
         sits on the page. Good typography is felt, not seen. A well-set page \
         draws the reader in without calling attention to itself, while a poorly \
         set one creates a subtle unease that disrupts the reading experience. \
         From Gutenberg to the present day, compositors have laboured over these \
         quiet refinements, adjusting the fit of each line so that the texture of \
         the text block remains even and undisturbed from margin to margin.";

    // Justified (default)
    doc.set_font("heading");
    doc.set_alignment(TextAlign::Left);
    doc.set_paragraph_indent(0.0);
    doc.add_text("Justified");
    doc.new_paragraph().expect("alignment heading 1");
    doc.add_vskip(4.0);

    doc.set_font("body");
    doc.set_paragraph_indent(20.0);
    doc.set_alignment(TextAlign::Justify);
    doc.add_text(demo_text);
    doc.new_paragraph().expect("justified paragraph");

    // Left-aligned
    doc.set_font("heading");
    doc.set_alignment(TextAlign::Left);
    doc.set_paragraph_indent(0.0);
    doc.add_text("Left-aligned");
    doc.new_paragraph().expect("alignment heading 2");
    doc.add_vskip(4.0);

    doc.set_font("body");
    doc.set_paragraph_indent(20.0);
    doc.set_alignment(TextAlign::Left);
    doc.add_text(demo_text);
    doc.new_paragraph().expect("left-aligned paragraph");

    // Right-aligned
    doc.set_font("heading");
    doc.set_alignment(TextAlign::Left);
    doc.set_paragraph_indent(0.0);
    doc.add_text("Right-aligned");
    doc.new_paragraph().expect("alignment heading 3");
    doc.add_vskip(4.0);

    doc.set_font("body");
    doc.set_paragraph_indent(0.0);
    doc.set_alignment(TextAlign::Right);
    doc.add_text(demo_text);
    doc.new_paragraph().expect("right-aligned paragraph");

    // Centered
    doc.set_font("heading");
    doc.set_alignment(TextAlign::Left);
    doc.set_paragraph_indent(0.0);
    doc.add_text("Centered");
    doc.new_paragraph().expect("alignment heading 4");
    doc.add_vskip(4.0);

    doc.set_font("body");
    doc.set_alignment(TextAlign::Center);
    doc.add_text(demo_text);
    doc.new_paragraph().expect("centered paragraph");

    // Reset for the following Urdu section
    doc.set_alignment(TextAlign::Justify);

    // --- Urdu section (Graphite via Awami Nastaliq) ---

    doc.add_vskip(18.0);

    // Load Awami Nastaliq — a Graphite-enabled font from SIL
    match find_awami_nastaliq() {
        None => println!("Skipping Urdu section: Awami Nastaliq not found (https://software.sil.org/awami/)"),
        Some(awami_path) => {
            println!("Using Nastaliq font: {}", awami_path.display());

            let awami_heading_spec = FontSpec {
                family: Some("Awami Nastaliq".to_string()),
                size: 22.0,
                direction: Direction::RTL,
                script: "Arab".to_string(),
                language: "ur".to_string(),
                ..Default::default()
            };
            doc.load_font_file("urdu-heading", &awami_path, awami_heading_spec)
                .expect("failed to load Awami Nastaliq heading");

            let awami_body_spec = FontSpec {
                family: Some("Awami Nastaliq".to_string()),
                size: 14.0,
                direction: Direction::RTL,
                script: "Arab".to_string(),
                language: "ur".to_string(),
                ..Default::default()
            };
            doc.load_font_file("urdu-body", &awami_path, awami_body_spec)
                .expect("failed to load Awami Nastaliq body");

            // Urdu heading
            doc.set_font("urdu-heading");
            doc.set_language("ur");
            doc.set_direction(Direction::RTL);
            doc.set_paragraph_indent(0.0);
            doc.set_leading(8.0);
            doc.add_text("\u{0628}\u{0648}\u{06C1}\u{06CC}\u{0645}\u{06CC}\u{0627} \u{0645}\u{06CC}\u{06BA} \u{0627}\u{06CC}\u{06A9} \u{0627}\u{0633}\u{06A9}\u{06CC}\u{0646}\u{0688}\u{0644}");
            // "بوہیمیا میں ایک اسکینڈل" = "A Scandal in Bohemia"
            doc.new_paragraph().expect("urdu title");

            doc.add_vskip(8.0);

            // Urdu body text — opening paragraph
            doc.set_font("urdu-body");
            doc.set_paragraph_indent(0.0);

            doc.add_text(
                "\u{0634}\u{0631}\u{0644}\u{0627}\u{06A9} \u{06C1}\u{0648}\u{0645}\u{0632} \u{06A9}\u{06CC} \u{0646}\u{0638}\u{0631} \u{0645}\u{06CC}\u{06BA} \u{0648}\u{06C1} \u{06C1}\u{0645}\u{06CC}\u{0634}\u{06C1} \u{0027}\u{0627}\u{0633} \u{0639}\u{0648}\u{0631}\u{062A}\u{0027} \u{062A}\u{06BE}\u{06CC}\u{06D4} \u{0645}\u{06CC}\u{06BA} \u{0646}\u{06D2} \u{0627}\u{0633}\u{06D2} \u{06A9}\u{0628}\u{06BE}\u{06CC} \u{06A9}\u{0645} \u{06C1}\u{06CC} \u{06A9}\u{0633}\u{06CC} \u{0627}\u{0648}\u{0631} \u{0646}\u{0627}\u{0645} \u{0633}\u{06D2} \u{0627}\u{0633} \u{06A9}\u{0627} \u{0630}\u{06A9}\u{0631} \u{06A9}\u{0631}\u{062A}\u{06D2} \u{0633}\u{0646}\u{0627} \u{06C1}\u{06D2}\u{06D4} \u{0627}\u{0633} \u{06A9}\u{06CC} \u{0646}\u{0638}\u{0631}\u{0648}\u{06BA} \u{0645}\u{06CC}\u{06BA} \u{0648}\u{06C1} \u{067E}\u{0648}\u{0631}\u{06CC} \u{0639}\u{0648}\u{0631}\u{062A} \u{0630}\u{0627}\u{062A} \u{067E}\u{0631} \u{0686}\u{06BE}\u{0627} \u{062C}\u{0627}\u{062A}\u{06CC} \u{06C1}\u{06D2}\u{06D4}",
            );
            // "شرلاک ہومز کی نظر میں وہ ہمیشہ 'اس عورت' تھی۔ میں نے اسے کبھی کم ہی کسی اور نام سے اس کا ذکر کرتے سنا ہے۔ اس کی نظروں میں وہ پوری عورت ذات پر چھا جاتی ہے۔"
            doc.new_paragraph().expect("urdu paragraph 1");

            doc.add_text(
                "\u{06CC}\u{06C1} \u{0627}\u{0633} \u{0644}\u{06CC}\u{06D2} \u{0646}\u{06C1}\u{06CC}\u{06BA} \u{062A}\u{06BE}\u{0627} \u{06A9}\u{06C1} \u{0627}\u{0633}\u{06D2} \u{0622}\u{0626}\u{0631}\u{06CC}\u{0646} \u{0627}\u{06CC}\u{0688}\u{0644}\u{0631} \u{0633}\u{06D2} \u{0645}\u{062D}\u{0628}\u{062A} \u{062C}\u{06CC}\u{0633}\u{06CC} \u{06A9}\u{0648}\u{0626}\u{06CC} \u{0686}\u{06CC}\u{0632} \u{062A}\u{06BE}\u{06CC}\u{06D4} \u{062A}\u{0645}\u{0627}\u{0645} \u{062C}\u{0630}\u{0628}\u{0627}\u{062A}\u{060C} \u{0627}\u{0648}\u{0631} \u{062E}\u{0627}\u{0635} \u{0637}\u{0648}\u{0631} \u{067E}\u{0631} \u{06CC}\u{06C1} \u{062C}\u{0630}\u{0628}\u{06C1}\u{060C} \u{0627}\u{0633} \u{06A9}\u{06D2} \u{0633}\u{0631}\u{062F} \u{0627}\u{0648}\u{0631} \u{062F}\u{0642}\u{06CC}\u{0642} \u{0644}\u{06CC}\u{06A9}\u{0646} \u{0642}\u{0627}\u{0628}\u{0644}\u{0650} \u{062A}\u{062D}\u{0633}\u{06CC}\u{0646} \u{0645}\u{062A}\u{0648}\u{0627}\u{0632}\u{0646} \u{0630}\u{06C1}\u{0646} \u{06A9}\u{06D2} \u{0644}\u{06CC}\u{06D2} \u{0646}\u{0627}\u{06AF}\u{0648}\u{0627}\u{0631} \u{062A}\u{06BE}\u{06D2}\u{06D4}",
            );
            // "یہ اس لیے نہیں تھا کہ اسے آئرین ایڈلر سے محبت جیسی کوئی چیز تھی۔ تمام جذبات، اور خاص طور پر یہ جذبہ، اس کے سرد اور دقیق لیکن قابلِ تحسین متوازن ذہن کے لیے ناگوار تھے۔"
            doc.new_paragraph().expect("urdu paragraph 2");
        }
    }

    // --- Render ---

    let pdf_bytes = doc.render().expect("render failed");

    std::fs::write(&output_path, &pdf_bytes).expect("failed to write PDF");

    println!(
        "Wrote {} bytes to {}",
        pdf_bytes.len(),
        output_path
    );
}

fn find_awami_nastaliq() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let candidates = [
        format!("{home}/Library/Fonts/AwamiNastaliq-Regular.ttf"),
        "/Library/Fonts/AwamiNastaliq-Regular.ttf".to_string(),
        "/usr/share/fonts/truetype/awami-nastaliq/AwamiNastaliq-Regular.ttf".to_string(),
    ];
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
}

fn load_system_font() -> Option<(Vec<u8>, String)> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    // Prefer common readable fonts
    let preferred = [
        "Gentium Plus",
        "Georgia",
        "Times New Roman",
        "DejaVu Serif",
        "Liberation Serif",
        "Noto Serif",
        "Palatino",
        "Book Antiqua",
    ];

    let face_id = preferred
        .iter()
        .find_map(|name| {
            db.faces()
                .find(|f| f.families.iter().any(|(fam, _)| fam == name))
                .map(|f| f.id)
        })
        .or_else(|| db.faces().next().map(|f| f.id))?;

    let family = db
        .faces()
        .find(|f| f.id == face_id)?
        .families
        .first()?
        .0
        .clone();

    let mut data_out: Option<Vec<u8>> = None;
    db.with_face_data(face_id, |data, _index| {
        data_out = Some(data.to_vec());
    });

    Some((data_out?, family))
}
