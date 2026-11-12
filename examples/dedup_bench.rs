//! Duplicate-detection benchmark (review N15).
//!
//! ```sh
//! cargo run --release --example dedup_bench -- [SCENARIO] [SIZE]...
//! ```
//!
//! Without arguments, runs every scenario at 1k, 5k, 10k and 20k entries.
//! For each library it reports the time to open it as `bibtui` does
//! (`App::new`: the time to a usable UI, less terminal setup; the default
//! duplicate check on load runs in the background and is not included),
//! the time of one full duplicate scan (as `U` runs, also in the
//! background), and the clusters found. Peak memory is printed on Unix (the process
//! maximum so far, so run one scenario and size per process to compare;
//! `run_all` below does that by re-invoking itself).

use std::path::Path;
use std::time::Instant;

use bibtui::app::App;
use bibtui::bib::dedup::{find_duplicates, DedupSettings};
use bibtui::config::defaults::default_config;

const SCENARIOS: [&str; 5] = [
    "unique-ids",
    "sparse",
    "similar-titles",
    "dense-clusters",
    "shared-words",
];
const SIZES: [usize; 4] = [1_000, 5_000, 10_000, 20_000];

/// Function words, which real titles repeat often.
const COMMON: [&str; 8] = ["of", "the", "for", "and", "in", "a", "on", "with"];

/// A small vocabulary: every title shares most of its q-grams with every
/// other, the worst case for the title filter.
const FEW_WORDS: [&str; 24] = [
    "neutron",
    "transport",
    "monte",
    "carlo",
    "methods",
    "analysis",
    "reactor",
    "physics",
    "variance",
    "reduction",
    "nuclear",
    "data",
    "benchmark",
    "criticality",
    "shielding",
    "simulation",
    "of",
    "the",
    "for",
    "and",
    "in",
    "a",
    "uncertainty",
    "validation",
];

const SYLLABLES: [&str; 32] = [
    "ra", "ne", "tor", "mi", "ca", "lo", "sim", "tra", "ver", "an", "ly", "sis", "phy", "co", "de",
    "ri", "va", "tion", "mo", "nu", "cle", "ar", "ben", "ch", "ga", "ma", "pro", "ba", "bi", "li",
    "ty", "en",
];

/// xorshift64
fn next(x: &mut u64) -> u64 {
    *x ^= *x << 13;
    *x ^= *x >> 7;
    *x ^= *x << 17;
    *x
}

/// One of ~32k pseudo-words, low numbers (common words) more likely.
fn word(x: &mut u64) -> String {
    if next(x).is_multiple_of(3) {
        return COMMON[(next(x) % COMMON.len() as u64) as usize].to_string();
    }
    // Skewed: the minimum of two draws favours a smaller vocabulary core.
    let n = (next(x) % 32_768).min(next(x) % 32_768);
    (0..3)
        .map(|s| SYLLABLES[((n >> (5 * s)) % 32) as usize])
        .collect()
}

/// A title of 5–10 words for `seed`, from realistic or `few` words.
fn title_from(seed: usize, few: bool) -> String {
    let mut x = seed as u64 * 0x9e37_79b9_7f4a_7c15 + 1;
    let words = 5 + (next(&mut x) % 6) as usize;
    (0..words)
        .map(|_| {
            if few {
                FEW_WORDS[(next(&mut x) % FEW_WORDS.len() as u64) as usize].to_string()
            } else {
                word(&mut x)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn title(seed: usize) -> String {
    title_from(seed, false)
}

fn library(scenario: &str, size: usize) -> String {
    (0..size)
        .map(|i| match scenario {
            // Every entry has its own DOI and a distinct title.
            "unique-ids" => format!(
                "@Article{{K{i}, title={{{} {i}}}, author={{Author{}, A.}}, year={{{}}}, doi={{10.1/{i}}}}}\n",
                title(i),
                i % 97,
                1990 + i % 30
            ),
            // Titles only: no year, author or identifier to separate them.
            "sparse" => format!("@Misc{{K{i}, title={{{}}}}}\n", title(i)),
            // Titles only, from 24 words: the title filter's worst case.
            "shared-words" => format!("@Misc{{K{i}, title={{{}}}}}\n", title_from(i, true)),
            // Short titles one character apart ("Row 00001", "Row 00002", ...).
            "similar-titles" => format!("@Misc{{K{i}, title={{Row {i:05}}}}}\n"),
            // Groups of 10 copies of one work sharing a DOI and title.
            "dense-clusters" => format!(
                "@Article{{K{i}, title={{{}}}, year={{2020}}, doi={{10.1/{}}}}}\n",
                title(i / 10),
                i / 10
            ),
            other => panic!("unknown scenario {other}"),
        })
        .collect()
}

#[cfg(unix)]
fn peak_memory_mib() -> Option<f64> {
    // SAFETY: getrusage only writes into the zeroed struct we pass.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    // ru_maxrss is in bytes on macOS and in KiB on Linux.
    let bytes = if cfg!(target_os = "macos") {
        usage.ru_maxrss as f64
    } else {
        usage.ru_maxrss as f64 * 1024.0
    };
    Some(bytes / (1024.0 * 1024.0))
}

#[cfg(not(unix))]
fn peak_memory_mib() -> Option<f64> {
    None
}

fn run(scenario: &str, size: usize, dir: &Path) -> anyhow::Result<()> {
    let path = dir.join(format!("{scenario}-{size}.bib"));
    std::fs::write(&path, library(scenario, size))?;

    let start = Instant::now();
    let app = App::new(path, default_config())?;
    let open = start.elapsed();
    let entries = app.database.entries.clone();
    // Dropping the app cancels its background load check, so the scan
    // below runs alone.
    drop(app);

    let start = Instant::now();
    let clusters = find_duplicates(&entries, &DedupSettings::default(), &[]);
    let scan = start.elapsed();

    let memory = peak_memory_mib()
        .map(|m| format!("{m:8.1} MiB"))
        .unwrap_or_else(|| "       n/a".into());
    println!(
        "{scenario:<15} {size:>6}  open {:>9.1} ms  scan {:>9.1} ms  {:>5} clusters  peak {memory}",
        open.as_secs_f64() * 1e3,
        scan.as_secs_f64() * 1e3,
        clusters.len(),
    );
    Ok(())
}

fn run_all() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    for scenario in SCENARIOS {
        for size in SIZES {
            let status = std::process::Command::new(&exe)
                .arg(scenario)
                .arg(size.to_string())
                .status()?;
            anyhow::ensure!(status.success(), "{scenario} {size} failed");
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((scenario, sizes)) = args.split_first() else {
        return run_all();
    };
    let sizes: Vec<usize> = if sizes.is_empty() {
        SIZES.to_vec()
    } else {
        sizes.iter().map(|s| s.parse()).collect::<Result<_, _>>()?
    };
    let dir = tempfile::tempdir()?;
    for size in sizes {
        run(scenario, size, dir.path())?;
    }
    Ok(())
}
