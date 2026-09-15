use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use flight_review::{cli_support as support, diagnostics, metadata, signal_processing};
use rayon::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, Args)]
struct Options {
    /// Run only this analyzer (see list); otherwise all analyzers run
    #[arg(long, value_name = "ID", conflicts_with = "exclude")]
    analyzer: Option<String>,
    /// Exclude analyzer IDs, comma-separated (see list)
    #[arg(long, value_name = "IDs", value_delimiter = ',')]
    exclude: Vec<String>,
    /// Parallel workers, 1..=256 (default: available CPUs, capped at 256)
    #[arg(long, short, value_parser = clap::value_parser!(u16).range(1..=256))]
    jobs: Option<u16>,
}

#[derive(Parser)]
#[command(
    name = "flight-review",
    version,
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true,
    about = "Analyze PX4 ULog flights; no files are written unless export is requested",
    after_help = "PATH may be a file or a directory (recursive, case-insensitive .ulg discovery).\n\
                  flight-review PATH [OPTIONS] is equivalent to flight-review analyze PATH [OPTIONS].\n\
                  Output is JSON; directories emit one JSON record per log (NDJSON) by default.\n\
                  All analyzers run by default, including PID step response; unavailable results explain why.\n\
                  Existing nonempty exports and overlapping input/output paths are refused."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// ULog file or directory to analyze
    #[arg(required = true)]
    path: Option<PathBuf>,
    #[command(flatten)]
    options: Options,
    /// Also export Parquet, metadata.json, and manifest.json to DIR
    #[arg(long, value_name = "DIR")]
    export_parquet: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze a file or recursively analyze a directory (the default action)
    Analyze {
        path: PathBuf,
        #[command(flatten)]
        options: Options,
        /// Also export Parquet, metadata.json, and manifest.json to DIR
        #[arg(long, value_name = "DIR")]
        export_parquet: Option<PathBuf>,
    },
    /// Explicitly convert a file or directory into a Parquet dataset
    Convert {
        path: PathBuf,
        /// Required export directory; existing nonempty directories are refused
        #[arg(long, short, value_name = "DIR")]
        output: PathBuf,
        #[command(flatten)]
        options: Options,
    },
    /// List all analyzer IDs for --analyzer and --exclude
    List,
}

struct Selection {
    diagnostics: Vec<String>,
    modules: Vec<String>,
}

impl Selection {
    fn resolve(options: &Options) -> Result<Self, String> {
        let diagnostics = diagnostics::create_analyzers();
        let modules = signal_processing::create_analyses();
        let mut ids: Vec<_> = diagnostics
            .iter()
            .map(|analyzer| analyzer.id())
            .chain(modules.iter().map(|analyzer| analyzer.id()))
            .collect();
        ids.sort_unstable();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("analyzer IDs must be unique across all registered analyzers".into());
        }
        for id in options.analyzer.iter().chain(&options.exclude) {
            if !ids.contains(&id.as_str()) {
                return Err(format!(
                    "unknown analyzer '{id}'. valid: {}",
                    ids.join(", ")
                ));
            }
        }
        let selected = |id: &str| match options.analyzer.as_deref() {
            Some(only) => only == id,
            None => !options.exclude.iter().any(|excluded| excluded == id),
        };
        let selection = Self {
            diagnostics: diagnostics
                .iter()
                .filter(|analyzer| selected(analyzer.id()))
                .map(|analyzer| analyzer.id().to_owned())
                .collect(),
            modules: modules
                .iter()
                .filter(|analyzer| selected(analyzer.id()))
                .map(|analyzer| analyzer.id().to_owned())
                .collect(),
        };
        if selection.diagnostics.is_empty() && selection.modules.is_empty() {
            return Err("no analyzers remain after exclusions".into());
        }
        Ok(selection)
    }
}

#[derive(Serialize)]
struct Summary {
    vehicle: Option<String>,
    hardware: Option<String>,
    duration_s: Option<f64>,
    topic_count: usize,
    message_count: usize,
    dropout_count: u32,
    dropout_total_ms: u64,
    completeness: metadata::Completeness,
}

#[derive(Serialize)]
struct DiagnosticOutcome {
    status: &'static str,
    message: String,
}

#[derive(Serialize)]
struct ModuleOutcome {
    status: &'static str,
    message: Option<String>,
    result: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct Record {
    version: u32,
    file: String,
    outcome: &'static str,
    summary: Option<Summary>,
    analysis: Option<flight_review::analysis::FlightAnalysis>,
    diagnostic_outcomes: BTreeMap<String, DiagnosticOutcome>,
    modules: BTreeMap<String, ModuleOutcome>,
    export: Option<PathBuf>,
    error: Option<String>,
}

impl Record {
    fn new(path: &Path, selection: &Selection) -> Self {
        Self {
            version: 1,
            file: path.to_string_lossy().into_owned(),
            outcome: "error",
            summary: None,
            analysis: None,
            diagnostic_outcomes: selection
                .diagnostics
                .iter()
                .map(|id| {
                    (
                        id.clone(),
                        DiagnosticOutcome {
                            status: "not_run",
                            message: "Log processing has not reached this analyzer.".into(),
                        },
                    )
                })
                .collect(),
            modules: selection
                .modules
                .iter()
                .map(|id| {
                    (
                        id.clone(),
                        ModuleOutcome {
                            status: "not_run",
                            message: Some("Log processing has not reached this analyzer.".into()),
                            result: None,
                        },
                    )
                })
                .collect(),
            export: None,
            error: None,
        }
    }
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        let _ = writeln!(std::io::stderr().lock(), "error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    // Options preceding an explicit command must not be silently ignored.
    if cli.command.is_some()
        && (cli.path.is_some()
            || cli.export_parquet.is_some()
            || cli.options.jobs.is_some()
            || !cli.options.exclude.is_empty()
            || cli.options.analyzer.is_some())
    {
        return Err("place options after the explicit subcommand".into());
    }
    let (path, options, output) = match cli.command {
        Some(Command::Analyze {
            path,
            options,
            export_parquet,
        }) => (path, options, export_parquet),
        Some(Command::Convert {
            path,
            options,
            output,
        }) => (path, options, Some(output)),
        Some(Command::List) => {
            #[derive(Serialize)]
            struct AnalyzerInfo {
                id: String,
                description: String,
            }
            #[derive(Serialize)]
            struct Catalog {
                version: u32,
                analyzers: Vec<AnalyzerInfo>,
            }
            let mut analyzers: Vec<_> = diagnostics::create_analyzers()
                .into_iter()
                .map(|analyzer| AnalyzerInfo {
                    id: analyzer.id().into(),
                    description: analyzer.description().into(),
                })
                .chain(
                    signal_processing::create_analyses()
                        .into_iter()
                        .map(|analyzer| AnalyzerInfo {
                            id: analyzer.id().into(),
                            description: analyzer.description().into(),
                        }),
                )
                .collect();
            analyzers.sort_by(|a, b| a.id.cmp(&b.id));
            let catalog = Catalog {
                version: 1,
                analyzers,
            };
            let mut stdout = std::io::stdout().lock();
            writeln!(stdout, "{}", support::json(&catalog, false)?).map_err(|e| e.to_string())?;
            return stdout.flush().map_err(|e| e.to_string());
        }
        None => (
            cli.path.ok_or("missing PATH")?,
            cli.options,
            cli.export_parquet,
        ),
    };
    let selection = Selection::resolve(&options)?;
    let pool = support::worker_pool(options.jobs.map(usize::from))?;
    let (directory, files) = match support::discover(&path) {
        Ok(discovered) => discovered,
        Err(error) => {
            let mut record = Record::new(&path, &selection);
            record.error = Some(error.clone());
            print_records(&[record])?;
            return Err(error);
        }
    };
    if let Some(output) = &output {
        if let Err(error) = support::ensure_output_safe(&path, output) {
            let mut record = Record::new(&path, &selection);
            record.error = Some(error.clone());
            print_records(&[record])?;
            return Err(error);
        }
    }
    let targets: Vec<Result<Option<PathBuf>, String>> = files
        .iter()
        .map(|file| {
            output
                .as_ref()
                .map(|output| {
                    if directory {
                        let relative = file.strip_prefix(&path).map_err(|e| e.to_string())?;
                        Ok(output.join(support::export_relative(relative)?))
                    } else {
                        Ok(output.clone())
                    }
                })
                .transpose()
        })
        .collect();

    let results: Vec<Record> = pool.install(|| {
        files
            .par_iter()
            .zip(&targets)
            .map(|(file, target)| {
                let mut record = Record::new(file, &selection);
                let processed = match target {
                    Ok(target) => process(file, target.as_deref(), &selection, &mut record),
                    Err(error) => Err(error.clone()),
                };
                if let Err(error) = processed {
                    record.error = Some(error);
                    record.outcome = "error";
                }
                if let Err(error) = support::json(&record, false) {
                    record.summary = None;
                    record.analysis = None;
                    record.modules.clear();
                    record.outcome = "error";
                    record.error = Some(format!("report serialization: {error}"));
                }
                record
            })
            .collect()
    });
    print_records(&results)?;
    if directory {
        if let Some(output) = &output {
            #[derive(Serialize)]
            struct IndexEntry<'a> {
                file: &'a str,
                outcome: &'a str,
                path: Option<PathBuf>,
                manifest: Option<PathBuf>,
                vehicle: Option<&'a str>,
                hardware: Option<&'a str>,
                duration_s: Option<f64>,
                diagnostic_count: usize,
                error: &'a Option<String>,
            }
            #[derive(Serialize)]
            struct Index<'a> {
                version: u32,
                total: usize,
                logs: Vec<IndexEntry<'a>>,
            }
            let logs = results
                .iter()
                .map(|record| {
                    let path = record
                        .export
                        .as_ref()
                        .and_then(|export| export.strip_prefix(output).ok().map(Path::to_owned));
                    let manifest = path.as_ref().map(|relative| relative.join("manifest.json"));
                    IndexEntry {
                        file: &record.file,
                        outcome: record.outcome,
                        path,
                        manifest,
                        vehicle: record
                            .summary
                            .as_ref()
                            .and_then(|summary| summary.vehicle.as_deref()),
                        hardware: record
                            .summary
                            .as_ref()
                            .and_then(|summary| summary.hardware.as_deref()),
                        duration_s: record
                            .summary
                            .as_ref()
                            .and_then(|summary| summary.duration_s),
                        diagnostic_count: record
                            .analysis
                            .as_ref()
                            .map_or(0, |analysis| analysis.diagnostics.len()),
                        error: &record.error,
                    }
                })
                .collect();
            std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
            support::write_json(
                &output.join("index.json"),
                &Index {
                    version: 1,
                    total: results.len(),
                    logs,
                },
                true,
            )?;
        }
    }
    let failures = results
        .iter()
        .filter(|record| record.error.is_some())
        .count();
    if failures > 0 {
        Err(format!("{failures} of {} logs failed", results.len()))
    } else {
        Ok(())
    }
}

fn process(
    path: &Path,
    output: Option<&Path>,
    selection: &Selection,
    record: &mut Record,
) -> Result<(), String> {
    let input = path.to_str().ok_or("input path is not valid UTF-8")?;
    let analyzers = diagnostics::create_analyzers_filtered(&selection.diagnostics)?;
    let mut metadata = if let Some(output) = output {
        let conversion =
            flight_review::converter::convert_ulog_with_analyzers(input, output, analyzers)
                .map_err(|e| e.to_string())?;
        support::write_json(&output.join("metadata.json"), &conversion.metadata, true)?;
        record.export = Some(output.to_owned());
        conversion.metadata
    } else {
        support::analyzed_metadata_with_analyzers(input, analyzers)?
    };
    record.summary = Some(Summary {
        vehicle: metadata.sys_name.clone(),
        hardware: metadata.ver_hw.clone(),
        duration_s: metadata.flight_duration_s,
        topic_count: metadata.topics.len(),
        message_count: metadata
            .topics
            .values()
            .map(|topic| topic.message_count)
            .sum(),
        dropout_count: metadata.dropout_count,
        dropout_total_ms: metadata.dropout_total_ms,
        completeness: metadata.completeness.clone(),
    });
    let mut analysis = metadata
        .analysis
        .take()
        .ok_or("flight analysis unavailable")?;
    analysis.diagnostics.sort_by(|a, b| {
        (a.timestamp_us, &a.id, a.anchor.instance).cmp(&(b.timestamp_us, &b.id, b.anchor.instance))
    });
    analysis
        .field_stats
        .sort_by(|a, b| (&a.topic, &a.field).cmp(&(&b.topic, &b.field)));
    analysis
        .non_default_params
        .sort_by(|a, b| a.name.cmp(&b.name));
    let analyzers = diagnostics::create_analyzers_filtered(&selection.diagnostics)?;
    for analyzer in analyzers {
        let found = analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == analyzer.id());
        let missing: Vec<_> = analyzer
            .required_topics()
            .iter()
            .filter(|topic| {
                metadata
                    .topics
                    .get(**topic)
                    .is_none_or(|info| info.message_count == 0)
            })
            .copied()
            .collect();
        let (status, message) = if found {
            (
                "findings",
                "Observed anomalies; see timestamps, anchors, and evidence.",
            )
        } else if !missing.is_empty() {
            (
                "unavailable",
                "Required topic data is missing; this check could not assess the flight.",
            )
        } else {
            ("no_findings", "No findings in available data; missing fields, insufficient data, or unmet conditions may limit this check. This is not a health guarantee.")
        };
        let message = if missing.is_empty() {
            message.into()
        } else {
            format!("{message} Missing topics: {}.", missing.join(", "))
        };
        record
            .diagnostic_outcomes
            .insert(analyzer.id().into(), DiagnosticOutcome { status, message });
    }
    record.analysis = Some(analysis);
    if !selection.modules.is_empty() {
        let modules = signal_processing::create_analyses_filtered(&selection.modules)?;
        let requests: HashSet<_> = modules
            .iter()
            .flat_map(|module| module.required_signals())
            .collect();
        let signals = match signal_processing::extract_signals(input, &requests) {
            Ok(signals) => signals,
            Err(error) => {
                for id in &selection.modules {
                    record.modules.insert(
                        id.clone(),
                        ModuleOutcome {
                            status: "error",
                            message: Some(error.to_string()),
                            result: None,
                        },
                    );
                }
                return Err(error.to_string());
            }
        };
        let mut failures = Vec::new();
        for module in modules {
            let outcome = match module.analyze(&signals) {
                Ok(result) => ModuleOutcome {
                    status: "completed",
                    message: None,
                    result: Some(result),
                },
                Err(signal_processing::AnalysisError::InsufficientData { reason }) => {
                    ModuleOutcome {
                        status: "unavailable",
                        message: Some(reason),
                        result: None,
                    }
                }
                Err(error) => {
                    failures.push(format!("{}: {error}", module.id()));
                    ModuleOutcome {
                        status: "error",
                        message: Some(error.to_string()),
                        result: None,
                    }
                }
            };
            record.modules.insert(module.id().into(), outcome);
        }
        if !failures.is_empty() {
            return Err(failures.join("; "));
        }
    }
    record.outcome = if output.is_some() {
        "exported"
    } else {
        "analyzed"
    };
    Ok(())
}

fn print_records(records: &[Record]) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    for record in records {
        let json = support::json(record, false)?;
        writeln!(stdout, "{json}").map_err(|e| e.to_string())?;
    }
    stdout.flush().map_err(|e| e.to_string())
}
