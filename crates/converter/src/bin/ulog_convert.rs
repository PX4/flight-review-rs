use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Pretty,
    Compact,
}

#[derive(Parser)]
#[command(name = "ulog-convert", about = "Convert ULog files to Parquet + metadata")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Input ULog file (when no subcommand is used)
    input: Option<String>,

    /// Output directory (default: <input_stem>_parquet)
    output_dir: Option<String>,

    /// Only extract metadata, skip Parquet conversion
    #[arg(long)]
    metadata_only: bool,

    /// Run PID step response analysis and output as JSON
    #[arg(long)]
    pid_analysis: bool,

    /// JSON output format for metadata.json
    #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
    output_format: OutputFormat,
}

#[derive(Subcommand)]
enum Command {
    /// List available diagnostic analyzers
    ListAnalyzers,

    /// Scan a directory of ULog files for diagnostics
    Scan {
        /// Directory containing .ulg files (searched recursively)
        path: String,

        /// Only show logs that have diagnostics
        #[arg(long)]
        diagnostics_only: bool,

        /// Run only specific analyzer(s), comma-separated
        #[arg(long, short, value_delimiter = ',')]
        analyzer: Vec<String>,

        /// Number of parallel workers (default: num CPUs)
        #[arg(long, short)]
        jobs: Option<usize>,

        /// Output format: pretty or compact JSON
        #[arg(long, value_enum, default_value_t = OutputFormat::Compact)]
        output_format: OutputFormat,
    },
}

fn serialize_metadata(
    metadata: &flight_review::metadata::FlightMetadata,
    format: &OutputFormat,
) -> String {
    match format {
        OutputFormat::Pretty => serde_json::to_string_pretty(metadata).unwrap(),
        OutputFormat::Compact => serde_json::to_string(metadata).unwrap(),
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::ListAnalyzers) => {
            let analyzers = flight_review::diagnostics::create_analyzers();
            for a in &analyzers {
                println!("{:<20} {}", a.id(), a.description());
                println!("{:<20} topics: {}", "", a.required_topics().join(", "));
                println!();
            }
            return;
        }
        Some(Command::Scan {
            path,
            diagnostics_only,
            analyzer,
            jobs,
            output_format,
        }) => {
            // Validate analyzer IDs upfront
            if !analyzer.is_empty() {
                if let Err(e) = flight_review::diagnostics::create_analyzers_filtered(&analyzer) {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
            run_scan(&path, diagnostics_only, &analyzer, jobs, &output_format);
            return;
        }
        None => {}
    }

    let input = match cli.input {
        Some(ref i) => i.as_str(),
        None => {
            eprintln!("error: missing input file");
            eprintln!("Usage: ulog-convert <INPUT> [OPTIONS]");
            eprintln!("       ulog-convert scan <DIR> [OPTIONS]");
            std::process::exit(1);
        }
    };

    if cli.pid_analysis {
        let result = match flight_review::pid_analysis::pid_analysis(input) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        };
        let json = match &cli.output_format {
            OutputFormat::Pretty => serde_json::to_string_pretty(&result).unwrap(),
            OutputFormat::Compact => serde_json::to_string(&result).unwrap(),
        };
        println!("{}", json);
        if !cli.metadata_only {
            return;
        }
    }

    if cli.metadata_only {
        let mut metadata = match flight_review::metadata::extract_metadata(input) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        };
        if let Ok(analysis) = flight_review::analysis::analyze(input, &metadata) {
            metadata.analysis = Some(analysis);
        }

        let json = serialize_metadata(&metadata, &cli.output_format);

        match &cli.output_dir {
            Some(dir) => {
                let output_path = Path::new(dir);
                std::fs::create_dir_all(output_path).unwrap();
                let meta_path = output_path.join("metadata.json");
                std::fs::write(&meta_path, &json).unwrap();
                eprintln!("Metadata written to {}", meta_path.display());
            }
            None => {
                println!("{}", json);
            }
        }
        return;
    }

    // Full conversion mode
    let output_dir = cli.output_dir.unwrap_or_else(|| {
        let stem = Path::new(input)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        format!("{}_parquet", stem)
    });

    let output_path = Path::new(&output_dir);

    let start = Instant::now();
    let result = match flight_review::converter::convert_ulog(input, output_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let elapsed = start.elapsed();

    let input_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let output_size: u64 = result
        .parquet_files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();

    eprintln!("Converted: {}", input);
    eprintln!("Output:    {} ({} files)", output_dir, result.parquet_files.len());
    eprintln!(
        "Size:      {:.1} MB -> {:.1} MB ({:.0}% of original)",
        input_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / input_size as f64 * 100.0
    );
    eprintln!("Time:      {:.0}ms", elapsed.as_millis());
    eprintln!(
        "Throughput: {:.0} MB/s",
        input_size as f64 / 1024.0 / 1024.0 / elapsed.as_secs_f64()
    );

    // Write metadata JSON
    let meta_path = output_path.join("metadata.json");
    let meta_json = serialize_metadata(&result.metadata, &cli.output_format);
    std::fs::write(&meta_path, &meta_json).unwrap();
    eprintln!("Metadata:  {}", meta_path.display());

    if let Some(name) = &result.metadata.sys_name {
        eprintln!(
            "\nVehicle:   {} ({})",
            name,
            result.metadata.ver_hw.as_deref().unwrap_or("unknown hw")
        );
    }
    eprintln!("Topics:    {}", result.metadata.topics.len());
    eprintln!(
        "Dropouts:  {} ({} ms total)",
        result.metadata.dropout_count, result.metadata.dropout_total_ms
    );
}

/// Scan result for a single ULog file.
#[derive(serde::Serialize)]
struct ScanResult {
    file: String,
    diagnostics: Vec<flight_review::diagnostics::Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn run_scan(dir: &str, diagnostics_only: bool, analyzer_filter: &[String], jobs: Option<usize>, format: &OutputFormat) {
    // Collect all .ulg files
    let files: Vec<String> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ulg"))
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    if files.is_empty() {
        eprintln!("No .ulg files found in {}", dir);
        std::process::exit(1);
    }

    let total = files.len();
    eprintln!("Scanning {} ULog files...", total);

    if let Some(j) = jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(j)
            .build_global()
            .ok();
    }

    let processed = AtomicUsize::new(0);
    let with_diags = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);

    let results: Vec<ScanResult> = files
        .par_iter()
        .filter_map(|file| {
            let result = scan_one_file(file, analyzer_filter);
            let n = processed.fetch_add(1, Ordering::Relaxed) + 1;

            if result.error.is_some() {
                errors.fetch_add(1, Ordering::Relaxed);
            }
            if !result.diagnostics.is_empty() {
                with_diags.fetch_add(1, Ordering::Relaxed);
            }

            // Progress on stderr every 50 files
            if n % 50 == 0 || n == total {
                eprintln!("  [{}/{}] processed", n, total);
            }

            if diagnostics_only && result.diagnostics.is_empty() && result.error.is_none() {
                return None;
            }

            Some(result)
        })
        .collect();

    // Output results
    for r in &results {
        let json = match format {
            OutputFormat::Pretty => serde_json::to_string_pretty(r).unwrap(),
            OutputFormat::Compact => serde_json::to_string(r).unwrap(),
        };
        println!("{}", json);
    }

    // Summary on stderr
    let diag_count = with_diags.load(Ordering::Relaxed);
    let err_count = errors.load(Ordering::Relaxed);
    eprintln!("\nSummary: {total} files scanned, {diag_count} with diagnostics, {err_count} errors");
}

fn scan_one_file(path: &str, analyzer_filter: &[String]) -> ScanResult {
    let metadata = match flight_review::metadata::extract_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return ScanResult {
                file: path.to_string(),
                diagnostics: vec![],
                vehicle: None,
                hardware: None,
                duration_s: None,
                error: Some(e.to_string()),
            };
        }
    };

    let analysis = match flight_review::analysis::analyze(path, &metadata) {
        Ok(a) => a,
        Err(e) => {
            return ScanResult {
                file: path.to_string(),
                diagnostics: vec![],
                vehicle: metadata.sys_name.clone(),
                hardware: metadata.ver_hw.clone(),
                duration_s: metadata.flight_duration_s,
                error: Some(e.to_string()),
            };
        }
    };

    let diagnostics = if analyzer_filter.is_empty() {
        analysis.diagnostics
    } else {
        analysis
            .diagnostics
            .into_iter()
            .filter(|d| analyzer_filter.iter().any(|id| id == &d.id))
            .collect()
    };

    ScanResult {
        file: path.to_string(),
        diagnostics,
        vehicle: metadata.sys_name.clone(),
        hardware: metadata.ver_hw.clone(),
        duration_s: metadata.flight_duration_s,
        error: None,
    }
}
