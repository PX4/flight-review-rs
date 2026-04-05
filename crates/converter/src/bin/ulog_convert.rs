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

#[derive(Debug, Clone, ValueEnum)]
enum BatchFormat {
    /// Human-readable summary table (default)
    Table,
    /// One JSON object per file, one per line
    Json,
    /// Pretty-printed JSON per file
    JsonPretty,
}

#[derive(Parser)]
#[command(name = "ulog-convert", version, about = "Convert and analyze PX4 ULog files")]
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
    /// Batch process a directory of ULog files
    ///
    /// Convert, diagnose, and analyze ULog files in parallel.
    ///
    /// Diagnostics: motor_failure, gps_interference, battery_brownout,
    /// ekf_failure, rc_loss
    ///
    /// Signal processing: pid_step_response
    Batch {
        /// Directory containing .ulg files (searched recursively)
        path: String,

        /// Output directory for Parquet + metadata (omit to skip conversion)
        #[arg(long, short)]
        output: Option<String>,

        /// Run diagnostic analyzers
        #[arg(long)]
        diagnostics: bool,

        /// Only show logs that have diagnostics (implies --diagnostics)
        #[arg(long)]
        diagnostics_only: bool,

        /// Filter to specific diagnostic analyzer(s), comma-separated
        #[arg(long, value_delimiter = ',')]
        analyzer: Vec<String>,

        /// Run signal processing analyses
        #[arg(long)]
        analyze: bool,

        /// Filter to specific analysis module(s), comma-separated
        #[arg(long, value_delimiter = ',')]
        modules: Vec<String>,

        /// Number of parallel workers (default: num CPUs)
        #[arg(long, short)]
        jobs: Option<usize>,

        /// Output format for results: table, json, or json-pretty
        #[arg(long, value_enum, default_value_t = BatchFormat::Table)]
        format: BatchFormat,
    },

    /// Run signal processing analyses on a single ULog file
    ///
    /// Modules: pid_step_response
    Analyze {
        /// Input ULog file
        file: String,

        /// Run only specific module(s), comma-separated
        #[arg(long, short, value_delimiter = ',')]
        modules: Vec<String>,

        /// Output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
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
        Some(Command::Analyze {
            file,
            modules,
            output_format,
        }) => {
            let analyses = if modules.is_empty() {
                flight_review::signal_processing::create_analyses()
            } else {
                match flight_review::signal_processing::create_analyses_filtered(&modules) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("error: {e}");
                        std::process::exit(1);
                    }
                }
            };

            match flight_review::signal_processing::run_analyses(&file, &analyses) {
                Ok(results) => {
                    let json = match output_format {
                        OutputFormat::Pretty => serde_json::to_string_pretty(&results).unwrap(),
                        OutputFormat::Compact => serde_json::to_string(&results).unwrap(),
                    };
                    println!("{json}");
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Batch {
            path,
            output,
            diagnostics,
            diagnostics_only,
            analyzer,
            analyze,
            modules,
            jobs,
            format,
        }) => {
            let opts = BatchOpts {
                convert: output.is_some(),
                output_dir: output,
                diagnostics: diagnostics || diagnostics_only || !analyzer.is_empty(),
                diagnostics_only,
                analyzer_filter: analyzer,
                analyze: analyze || !modules.is_empty(),
                module_filter: modules,
            };

            // Validate filters upfront
            if !opts.analyzer_filter.is_empty() {
                if let Err(e) =
                    flight_review::diagnostics::create_analyzers_filtered(&opts.analyzer_filter)
                {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }
            if !opts.module_filter.is_empty() {
                if let Err(e) =
                    flight_review::signal_processing::create_analyses_filtered(&opts.module_filter)
                {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            }

            // Default: if nothing specified, just convert
            let opts = if !opts.convert && !opts.diagnostics && !opts.analyze {
                eprintln!("hint: use -o <DIR> to convert, --diagnostics to scan, --analyze for signal processing");
                eprintln!();
                BatchOpts {
                    diagnostics: true,
                    diagnostics_only: true,
                    ..opts
                }
            } else {
                opts
            };

            run_batch(&path, &opts, jobs, &format);
            return;
        }
        None => {}
    }

    // --- Single-file mode (no subcommand) ---

    let input = match cli.input {
        Some(ref i) => i.as_str(),
        None => {
            eprintln!("error: missing input file");
            eprintln!();
            eprintln!("Usage:");
            eprintln!("  ulog-convert <FILE> [OPTIONS]           Convert a single file");
            eprintln!("  ulog-convert batch <DIR> [OPTIONS]      Batch process a directory");
            eprintln!("  ulog-convert analyze <FILE> [OPTIONS]   Run signal processing");
            eprintln!("  ulog-convert list                       List available modules");
            std::process::exit(1);
        }
    };

    if cli.pid_analysis {
        let result = match flight_review::pid_analysis::pid_analysis(input) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
        let json = match &cli.output_format {
            OutputFormat::Pretty => serde_json::to_string_pretty(&result).unwrap(),
            OutputFormat::Compact => serde_json::to_string(&result).unwrap(),
        };
        println!("{json}");
        if !cli.metadata_only {
            return;
        }
    }

    if cli.metadata_only {
        let mut metadata = match flight_review::metadata::extract_metadata(input) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {e}");
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
                println!("{json}");
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
        format!("{stem}_parquet")
    });

    let output_path = Path::new(&output_dir);

    let start = Instant::now();
    let result = match flight_review::converter::convert_ulog(input, output_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
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

    eprintln!("Converted: {input}");
    eprintln!(
        "Output:    {output_dir} ({} files)",
        result.parquet_files.len()
    );
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

// ---------------------------------------------------------------------------
// Batch processing
// ---------------------------------------------------------------------------

struct BatchOpts {
    convert: bool,
    output_dir: Option<String>,
    diagnostics: bool,
    diagnostics_only: bool,
    analyzer_filter: Vec<String>,
    analyze: bool,
    module_filter: Vec<String>,
}

#[derive(serde::Serialize)]
struct BatchResult {
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    converted: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<flight_review::diagnostics::Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    analyses: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn run_batch(dir: &str, opts: &BatchOpts, jobs: Option<usize>, format: &BatchFormat) {
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
        eprintln!("No .ulg files found in {dir}");
        std::process::exit(1);
    }

    let total = files.len();
    let mut actions = Vec::new();
    if opts.convert {
        actions.push("convert");
    }
    if opts.diagnostics {
        actions.push("diagnostics");
    }
    if opts.analyze {
        actions.push("analyze");
    }
    eprintln!(
        "Processing {} ULog files [{}]...\n",
        total,
        actions.join(", ")
    );

    if let Some(j) = jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(j)
            .build_global()
            .ok();
    }

    let processed = AtomicUsize::new(0);
    let with_diags = AtomicUsize::new(0);
    let converted = AtomicUsize::new(0);
    let errors = AtomicUsize::new(0);

    let results: Vec<BatchResult> = files
        .par_iter()
        .filter_map(|file| {
            let result = process_one_file(file, opts);
            let n = processed.fetch_add(1, Ordering::Relaxed) + 1;

            if result.error.is_some() {
                errors.fetch_add(1, Ordering::Relaxed);
            }
            if !result.diagnostics.is_empty() {
                with_diags.fetch_add(1, Ordering::Relaxed);
            }
            if result.converted == Some(true) {
                converted.fetch_add(1, Ordering::Relaxed);
            }

            if n.is_multiple_of(100) || n == total {
                eprint!("\r  [{n}/{total}] processed");
            }

            // Filter out empty results if diagnostics-only
            if opts.diagnostics_only
                && result.diagnostics.is_empty()
                && result.error.is_none()
            {
                return None;
            }

            Some(result)
        })
        .collect();

    eprintln!();

    // Output
    match format {
        BatchFormat::Table => print_table(&results, opts),
        BatchFormat::Json => {
            for r in &results {
                println!("{}", serde_json::to_string(r).unwrap());
            }
        }
        BatchFormat::JsonPretty => {
            for r in &results {
                println!("{}", serde_json::to_string_pretty(r).unwrap());
            }
        }
    }

    // Summary
    let diag_count = with_diags.load(Ordering::Relaxed);
    let conv_count = converted.load(Ordering::Relaxed);
    let err_count = errors.load(Ordering::Relaxed);
    let mut parts = vec![format!("{total} files")];
    if opts.convert {
        parts.push(format!("{conv_count} converted"));
    }
    if opts.diagnostics {
        parts.push(format!("{diag_count} with diagnostics"));
    }
    parts.push(format!("{err_count} errors"));
    eprintln!("\n{}", parts.join(", "));
}

fn process_one_file(path: &str, opts: &BatchOpts) -> BatchResult {
    let metadata = match flight_review::metadata::extract_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return BatchResult {
                file: path.to_string(),
                converted: None,
                diagnostics: vec![],
                analyses: None,
                vehicle: None,
                hardware: None,
                duration_s: None,
                error: Some(e.to_string()),
            };
        }
    };

    let vehicle = metadata.sys_name.clone();
    let hardware = metadata.ver_hw.clone();
    let duration_s = metadata.flight_duration_s;

    // Conversion
    let mut did_convert = None;
    if opts.convert {
        if let Some(ref output_dir) = opts.output_dir {
            let stem = Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let file_output = Path::new(output_dir).join(&stem);
            match flight_review::converter::convert_ulog(path, &file_output) {
                Ok(result) => {
                    let meta_json = serde_json::to_string_pretty(&result.metadata).unwrap();
                    let _ = std::fs::write(file_output.join("metadata.json"), &meta_json);
                    did_convert = Some(true);
                }
                Err(e) => {
                    return BatchResult {
                        file: path.to_string(),
                        converted: Some(false),
                        diagnostics: vec![],
                        analyses: None,
                        vehicle,
                        hardware,
                        duration_s,
                        error: Some(e.to_string()),
                    };
                }
            }
        }
    }

    // Diagnostics
    let diagnostics = if opts.diagnostics {
        match flight_review::analysis::analyze(path, &metadata) {
            Ok(analysis) => {
                if opts.analyzer_filter.is_empty() {
                    analysis.diagnostics
                } else {
                    analysis
                        .diagnostics
                        .into_iter()
                        .filter(|d| opts.analyzer_filter.iter().any(|id| id == &d.id))
                        .collect()
                }
            }
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    // Signal processing
    let analyses = if opts.analyze {
        let modules = if opts.module_filter.is_empty() {
            flight_review::signal_processing::create_analyses()
        } else {
            flight_review::signal_processing::create_analyses_filtered(&opts.module_filter)
                .unwrap_or_default()
        };
        match flight_review::signal_processing::run_analyses(path, &modules) {
            Ok(r) if !r.is_empty() => Some(r),
            _ => None,
        }
    } else {
        None
    };

    BatchResult {
        file: path.to_string(),
        converted: did_convert,
        diagnostics,
        analyses,
        vehicle,
        hardware,
        duration_s,
        error: None,
    }
}

fn print_table(results: &[BatchResult], opts: &BatchOpts) {
    if results.is_empty() {
        println!("No results.");
        return;
    }

    // Dynamic header based on what was requested
    let mut header = format!("{:<44} {:<16} {:<10}", "FILE", "VEHICLE", "HARDWARE");
    if opts.convert {
        header.push_str(" CONV");
    }
    if opts.diagnostics {
        header.push_str(" DIAGNOSTICS");
    }
    println!("{header}");
    println!("{}", "-".repeat(header.len().max(110)));

    for r in results {
        let mut line = format!(
            "{:<44} {:<16} {:<10}",
            truncate_path(&r.file, 44),
            r.vehicle.as_deref().unwrap_or("-"),
            r.hardware.as_deref().unwrap_or("-"),
        );

        if let Some(ref err) = r.error {
            line.push_str(&format!(" ERROR: {err}"));
            println!("{line}");
            continue;
        }

        if opts.convert {
            match r.converted {
                Some(true) => line.push_str("  ok "),
                Some(false) => line.push_str(" FAIL"),
                None => line.push_str("  -  "),
            }
        }

        if opts.diagnostics {
            if r.diagnostics.is_empty() {
                line.push_str(" -");
            } else {
                let mut counts: Vec<(String, usize)> = Vec::new();
                for d in &r.diagnostics {
                    if let Some(entry) = counts.iter_mut().find(|(id, _)| id == &d.id) {
                        entry.1 += 1;
                    } else {
                        counts.push((d.id.clone(), 1));
                    }
                }
                let diag_str: Vec<String> = counts
                    .iter()
                    .map(|(id, n)| {
                        if *n > 1 {
                            format!("{id}({n})")
                        } else {
                            id.clone()
                        }
                    })
                    .collect();
                let worst = if r.diagnostics.iter().any(|d| {
                    matches!(d.severity, flight_review::diagnostics::Severity::Critical)
                }) {
                    "!"
                } else {
                    " "
                };
                line.push_str(&format!(" {worst} {}", diag_str.join(", ")));
            }
        }

        println!("{line}");
    }
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let parts: Vec<&str> = path.rsplit('/').take(2).collect();
    let short = format!(
        ".../{}",
        parts.into_iter().rev().collect::<Vec<_>>().join("/")
    );
    if short.len() <= max_len {
        short
    } else {
        format!("...{}", &path[path.len() - (max_len - 3)..])
    }
}
