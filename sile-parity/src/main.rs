mod compare;
mod driver;
mod fonts;
mod report;
mod sil;
mod svg;
mod trace;

use std::path::PathBuf;

use compare::Comparison;
use driver::{Corpus, Failure};

pub enum Outcome {
    Compared {
        ours: trace::Trace,
        comparison: Comparison,
    },
    Unsupported(Vec<String>),
    Error(String),
}

pub struct TestResult {
    pub name: String,
    pub expected: trace::Trace,
    pub outcome: Outcome,
}

const USAGE: &str = "usage: sile-parity [--corpus DIR] [--out DIR] [--trace TEST] [FILTER...]

Runs SILE's regression tests through sile-core and compares the layouts with
SILE's expected debug output. Fetch the corpus first with
scripts/fetch-parity-corpus.sh.

  --corpus DIR   corpus fetched by the script (default: .parity)
  --out DIR      where to write the HTML report (default: target/parity)
  --trace TEST   print our trace for one test and exit
  FILTER         only run tests whose name contains one of these strings";

fn main() {
    let mut corpus_dir = PathBuf::from(".parity");
    let mut out_dir = PathBuf::from("target/parity");
    let mut trace_only = None;
    let mut filters = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--corpus" => corpus_dir = args.next().map(PathBuf::from).unwrap_or(corpus_dir),
            "--out" => out_dir = args.next().map(PathBuf::from).unwrap_or(out_dir),
            "--trace" => trace_only = args.next(),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            f => filters.push(f.to_string()),
        }
    }

    let tests_dir = corpus_dir.join("sile/tests");
    let fonts = fonts::Fonts::load(&corpus_dir.join("fonts"));
    if !tests_dir.is_dir() || fonts.is_empty() {
        eprintln!(
            "corpus not found in {}; run scripts/fetch-parity-corpus.sh",
            corpus_dir.display()
        );
        std::process::exit(2);
    }
    let lorem_lua = std::fs::read_to_string(corpus_dir.join("sile/packages/lorem/init.lua"))
        .unwrap_or_default();
    let lorem = driver::lorem_source(&lorem_lua);
    let corpus = Corpus {
        fonts: &fonts,
        lorem: &lorem,
    };

    if let Some(name) = trace_only {
        let src =
            std::fs::read_to_string(tests_dir.join(format!("{name}.sil"))).expect("read test");
        match driver::run(&src, &corpus) {
            Ok(t) => print!("{t}"),
            Err(e) => {
                eprintln!("{e:?}");
                std::process::exit(1);
            }
        }
        return;
    }

    let mut names: Vec<String> = std::fs::read_dir(&tests_dir)
        .expect("read tests dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".expected")
                .map(str::to_string)
        })
        .filter(|n| filters.is_empty() || filters.iter().any(|f| n.contains(f.as_str())))
        .collect();
    names.sort();

    std::panic::set_hook(Box::new(|_| {}));
    let results: Vec<TestResult> = names
        .iter()
        .map(|name| run_one(name, &tests_dir, &corpus))
        .collect();
    let _ = std::panic::take_hook();

    report::print_summary(&results);
    match report::write_html(&results, &fonts, &out_dir) {
        Ok(index) => println!("\nreport: {}", index.display()),
        Err(e) => eprintln!("could not write report: {e}"),
    }
}

fn run_one(name: &str, dir: &std::path::Path, corpus: &Corpus) -> TestResult {
    let expected_src =
        std::fs::read_to_string(dir.join(format!("{name}.expected"))).unwrap_or_default();
    let mut expected = trace::parse(&expected_src);
    corpus.fonts.fill_advances(&mut expected);
    let sil = dir.join(format!("{name}.sil"));
    let outcome = if !sil.exists() {
        Outcome::Unsupported(vec!["XML input".to_string()])
    } else {
        let src = std::fs::read_to_string(&sil).unwrap_or_default();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| driver::run(&src, corpus))) {
            Ok(Ok(ours)) => {
                let mut ours = trace::parse(&ours);
                corpus.fonts.fill_advances(&mut ours);
                let comparison = compare::compare(&expected, &ours);
                Outcome::Compared { ours, comparison }
            }
            Ok(Err(Failure::Unsupported(missing))) => {
                Outcome::Unsupported(missing.into_iter().collect())
            }
            Ok(Err(Failure::Error(e))) => Outcome::Error(e),
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                Outcome::Error(format!("panic: {msg}"))
            }
        }
    };
    TestResult {
        name: name.to_string(),
        expected,
        outcome,
    }
}
