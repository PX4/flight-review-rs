use std::io::Write;
use std::path::Path;
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
#[command(
    name = "ulog-convert",
    version,
    about = "Convert and analyze PX4 ULog files"
)]
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

        /// Number of parallel workers, 1–256 (default: num CPUs)
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

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = run() {
        let _ = writeln!(std::io::stderr().lock(), "error: {error}");
        std::process::exit(1);
    }
}

fn run() -> CliResult<()> {
    let cli = Cli::parse();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();

    match cli.command {
        Some(Command::Analyze {
            file,
            modules,
            output_format,
        }) => {
            let analyses = if modules.is_empty() {
                flight_review::signal_processing::create_analyses()
            } else {
                flight_review::signal_processing::create_analyses_filtered(&modules)?
            };

            let results = flight_review::signal_processing::run_analyses(&file, &analyses)?;
            let json = flight_review::cli_support::json(
                &results,
                matches!(output_format, OutputFormat::Pretty),
            )?;
            writeln!(stdout, "{json}")?;
            return Ok(());
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
                flight_review::diagnostics::create_analyzers_filtered(&opts.analyzer_filter)?;
            }
            if !opts.module_filter.is_empty() {
                flight_review::signal_processing::create_analyses_filtered(&opts.module_filter)?;
            }

            // Preserve the legacy diagnostics-only default when no action is specified.
            let opts = if !opts.convert && !opts.diagnostics && !opts.analyze {
                writeln!(stderr, "hint: use -o <DIR> to convert, --diagnostics to scan, --analyze for signal processing\n")?;
                BatchOpts {
                    diagnostics: true,
                    diagnostics_only: true,
                    ..opts
                }
            } else {
                opts
            };

            return run_batch(&path, &opts, jobs, &format, &mut stdout, &mut stderr);
        }
        None => {}
    }

    // --- Single-file mode (no subcommand) ---

    let input = match cli.input {
        Some(ref i) => i.as_str(),
        None => {
            return Err("missing input file\n\nUsage:\n  ulog-convert <FILE> [OPTIONS]           Convert a single file\n  ulog-convert batch <DIR> [OPTIONS]      Batch process a directory\n  ulog-convert analyze <FILE> [OPTIONS]   Run signal processing\n  flight-review list                     List available modules".into());
        }
    };

    if cli.pid_analysis {
        let result = flight_review::pid_analysis::pid_analysis(input)?;
        let json = flight_review::cli_support::json(
            &result,
            matches!(cli.output_format, OutputFormat::Pretty),
        )?;
        writeln!(stdout, "{json}")?;
        if !cli.metadata_only {
            return Ok(());
        }
    }

    if cli.metadata_only {
        let metadata = flight_review::cli_support::analyzed_metadata(input)?;
        let json = flight_review::cli_support::json(
            &metadata,
            matches!(cli.output_format, OutputFormat::Pretty),
        )?;

        match &cli.output_dir {
            Some(dir) => {
                let output_path = Path::new(dir);
                std::fs::create_dir_all(output_path)?;
                let meta_path = output_path.join("metadata.json");
                std::fs::write(&meta_path, &json)?;
                writeln!(stderr, "Metadata written to {}", meta_path.display())?;
            }
            None => {
                writeln!(stdout, "{json}")?;
            }
        }
        return Ok(());
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
    let result = flight_review::converter::convert_ulog(input, output_path)?;
    let elapsed = start.elapsed();

    let input_size = std::fs::metadata(input)?.len();
    let output_size: u64 = result
        .parquet_files
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum();

    writeln!(stderr, "Converted: {input}")?;
    writeln!(
        stderr,
        "Output:    {output_dir} ({} files)",
        result.parquet_files.len()
    )?;
    writeln!(
        stderr,
        "Size:      {:.1} MB -> {:.1} MB ({:.0}% of original)",
        input_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / input_size.max(1) as f64 * 100.0
    )?;
    writeln!(stderr, "Time:      {:.0}ms", elapsed.as_millis())?;
    writeln!(
        stderr,
        "Throughput: {:.0} MB/s",
        input_size as f64 / 1024.0 / 1024.0 / elapsed.as_secs_f64().max(f64::EPSILON)
    )?;

    let meta_path = output_path.join("metadata.json");
    flight_review::cli_support::write_json(
        &meta_path,
        &result.metadata,
        matches!(cli.output_format, OutputFormat::Pretty),
    )?;
    writeln!(stderr, "Metadata:  {}", meta_path.display())?;

    if let Some(name) = &result.metadata.sys_name {
        writeln!(
            stderr,
            "\nVehicle:   {} ({})",
            name,
            result.metadata.ver_hw.as_deref().unwrap_or("unknown hw")
        )?;
    }
    writeln!(stderr, "Topics:    {}", result.metadata.topics.len())?;
    writeln!(
        stderr,
        "Dropouts:  {} ({} ms total)",
        result.metadata.dropout_count, result.metadata.dropout_total_ms
    )?;
    Ok(())
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
    #[serde(skip)]
    output_dir: Option<String>,
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

/// Top-level index for batch conversions.
#[derive(serde::Serialize)]
struct BatchIndex {
    version: u32,
    total: usize,
    logs: Vec<BatchIndexEntry>,
}

#[derive(serde::Serialize)]
struct BatchIndexEntry {
    path: String,
    manifest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_s: Option<f64>,
    diagnostic_count: usize,
}

fn run_batch(
    dir: &str,
    opts: &BatchOpts,
    jobs: Option<usize>,
    format: &BatchFormat,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> CliResult<()> {
    let pool = flight_review::cli_support::worker_pool(jobs)?;
    let (_, discovered) = flight_review::cli_support::discover(Path::new(dir))?;
    let files: Vec<String> = discovered
        .iter()
        .map(|path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("input path is not valid UTF-8: {}", path.display()))
        })
        .collect::<Result<_, _>>()?;

    if files.is_empty() {
        return Err(format!("No .ulg files found in {dir}").into());
    }

    if opts.convert {
        let mut targets = std::collections::HashMap::new();
        for file in &files {
            let stem = output_stem(file)?;
            // Also protect case-insensitive filesystems without changing the legacy layout.
            if let Some(previous) = targets.insert(stem.to_lowercase(), file) {
                return Err(format!(
                    "colliding batch output target '{stem}': {previous} and {file}"
                )
                .into());
            }
        }
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
    writeln!(
        stderr,
        "Processing {} ULog files [{}]...\n",
        total,
        actions.join(", ")
    )?;

    let results: Vec<BatchResult> = pool.install(|| {
        files
            .par_iter()
            .map(|file| process_one_file(file, opts))
            .collect()
    });
    let visible: Vec<&BatchResult> = results
        .iter()
        .filter(|result| {
            !opts.diagnostics_only || !result.diagnostics.is_empty() || result.error.is_some()
        })
        .collect();

    // Output
    match format {
        BatchFormat::Table => print_table(&visible, opts, stdout)?,
        BatchFormat::Json => {
            for r in &visible {
                writeln!(stdout, "{}", flight_review::cli_support::json(r, false)?)?;
            }
        }
        BatchFormat::JsonPretty => {
            for r in &visible {
                writeln!(stdout, "{}", flight_review::cli_support::json(r, true)?)?;
            }
        }
    }

    // Write batch-level index.json if converting
    if let Some(ref output_dir) = opts.output_dir {
        let entries: Vec<BatchIndexEntry> = results
            .iter()
            .filter(|r| r.converted == Some(true))
            .filter_map(|r| {
                let dir_name = r.output_dir.as_ref()?;
                Some(BatchIndexEntry {
                    path: dir_name.clone(),
                    manifest: format!("{dir_name}/manifest.json"),
                    vehicle: r.vehicle.clone(),
                    hardware: r.hardware.clone(),
                    duration_s: r.duration_s,
                    diagnostic_count: r.diagnostics.len(),
                })
            })
            .collect();

        let index = BatchIndex {
            version: 1,
            total: entries.len(),
            logs: entries,
        };

        let index_path = Path::new(output_dir).join("index.json");
        std::fs::create_dir_all(output_dir)?;
        flight_review::cli_support::write_json(&index_path, &index, true)?;
        writeln!(stderr, "Index:  {}", index_path.display())?;
    }

    // Summary
    let diag_count = results.iter().filter(|r| !r.diagnostics.is_empty()).count();
    let conv_count = results.iter().filter(|r| r.converted == Some(true)).count();
    let err_count = results.iter().filter(|r| r.error.is_some()).count();
    let mut parts = vec![format!("{total} files")];
    if opts.convert {
        parts.push(format!("{conv_count} converted"));
    }
    if opts.diagnostics {
        parts.push(format!("{diag_count} with diagnostics"));
    }
    parts.push(format!("{err_count} errors"));
    writeln!(stderr, "\n{}", parts.join(", "))?;
    if err_count > 0 {
        return Err(format!("{err_count} ULog file(s) failed").into());
    }
    Ok(())
}

fn output_stem(path: &str) -> Result<String, String> {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("invalid output stem for {path}"))
}

fn process_one_file(path: &str, opts: &BatchOpts) -> BatchResult {
    let mut result = BatchResult {
        file: path.to_string(),
        converted: opts.convert.then_some(false),
        output_dir: None,
        diagnostics: vec![],
        analyses: None,
        vehicle: None,
        hardware: None,
        duration_s: None,
        error: None,
    };
    if let Err(error) = process_file(path, opts, &mut result) {
        result.error = Some(error.to_string());
    }
    // Keep serialization failures representable as per-file JSON error records.
    if let Err(error) = flight_review::cli_support::json(&result, false) {
        result.error = Some(match result.error.take() {
            Some(previous) => format!("{previous}; {error}"),
            None => error,
        });
        result.duration_s = None;
        result.diagnostics.clear();
        result.analyses = None;
    }
    result
}

fn process_file(path: &str, opts: &BatchOpts, result: &mut BatchResult) -> CliResult<()> {
    let mut metadata = if let Some(output_dir) = opts.output_dir.as_ref().filter(|_| opts.convert) {
        let stem = output_stem(path)?;
        let file_output = Path::new(output_dir).join(&stem);
        let conversion = flight_review::converter::convert_ulog(path, &file_output)?;
        result.vehicle = conversion.metadata.sys_name.clone();
        result.hardware = conversion.metadata.ver_hw.clone();
        result.duration_s = conversion.metadata.flight_duration_s;
        flight_review::cli_support::write_json(
            &file_output.join("metadata.json"),
            &conversion.metadata,
            true,
        )?;
        result.converted = Some(true);
        result.output_dir = Some(stem);
        conversion.metadata
    } else if opts.diagnostics {
        flight_review::cli_support::analyzed_metadata(path)?
    } else {
        flight_review::metadata::extract_metadata(path)?
    };

    result.vehicle = metadata.sys_name.clone();
    result.hardware = metadata.ver_hw.clone();
    result.duration_s = metadata.flight_duration_s;

    if opts.diagnostics {
        let analysis = metadata
            .analysis
            .take()
            .ok_or("flight analysis is missing from metadata")?;
        result.diagnostics = analysis
            .diagnostics
            .into_iter()
            .filter(|d| {
                opts.analyzer_filter.is_empty() || opts.analyzer_filter.iter().any(|id| id == &d.id)
            })
            .collect();
    }

    if opts.analyze {
        let modules = if opts.module_filter.is_empty() {
            flight_review::signal_processing::create_analyses()
        } else {
            flight_review::signal_processing::create_analyses_filtered(&opts.module_filter)?
        };
        let analyses = flight_review::signal_processing::run_analyses(path, &modules)?;
        if !analyses.is_empty() {
            result.analyses = Some(analyses);
        }
    }
    Ok(())
}

fn print_table(
    results: &[&BatchResult],
    opts: &BatchOpts,
    stdout: &mut impl Write,
) -> CliResult<()> {
    if results.is_empty() {
        writeln!(stdout, "No results.")?;
        return Ok(());
    }

    // Dynamic header based on what was requested
    let mut header = format!("{:<44} {:<16} {:<10}", "FILE", "VEHICLE", "HARDWARE");
    if opts.convert {
        header.push_str(" CONV");
    }
    if opts.diagnostics {
        header.push_str(" DIAGNOSTICS");
    }
    writeln!(stdout, "{header}")?;
    writeln!(stdout, "{}", "-".repeat(header.len().max(110)))?;

    for r in results {
        let mut line = format!(
            "{:<44} {:<16} {:<10}",
            truncate_path(&r.file, 44),
            r.vehicle.as_deref().unwrap_or("-"),
            r.hardware.as_deref().unwrap_or("-"),
        );

        if let Some(ref err) = r.error {
            line.push_str(&format!(" ERROR: {err}"));
            writeln!(stdout, "{line}")?;
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
                let worst =
                    if r.diagnostics.iter().any(|d| {
                        matches!(d.severity, flight_review::diagnostics::Severity::Critical)
                    }) {
                        "!"
                    } else {
                        " "
                    };
                line.push_str(&format!(" {worst} {}", diag_str.join(", ")));
            }
        }

        writeln!(stdout, "{line}")?;
    }
    Ok(())
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
        let start = path
            .char_indices()
            .rev()
            .nth(max_len.saturating_sub(4))
            .map_or(0, |(index, _)| index);
        format!("...{}", &path[start..])
    }
}
