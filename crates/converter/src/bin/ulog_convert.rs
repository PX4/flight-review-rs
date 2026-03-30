use std::path::Path;
use std::time::Instant;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Pretty,
    Compact,
}

#[derive(Parser)]
#[command(name = "ulog-convert", about = "Convert ULog files to Parquet + metadata")]
struct Cli {
    /// Input ULog file
    input: String,

    /// Output directory (default: <input_stem>_parquet)
    output_dir: Option<String>,

    /// Only extract metadata, skip Parquet conversion
    #[arg(long)]
    metadata_only: bool,

    /// JSON output format for metadata.json
    #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
    output_format: OutputFormat,
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

    if cli.metadata_only {
        let mut metadata = match flight_review::metadata::extract_metadata(&cli.input) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        };
        if let Ok(analysis) = flight_review::analysis::analyze(&cli.input, &metadata) {
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
        let stem = Path::new(&cli.input)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        format!("{}_parquet", stem)
    });

    let output_path = Path::new(&output_dir);

    let start = Instant::now();
    let result = match flight_review::converter::convert_ulog(&cli.input, output_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let elapsed = start.elapsed();

    let input_size = std::fs::metadata(&cli.input).map(|m| m.len()).unwrap_or(0);
    let output_size: u64 = result
        .parquet_files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();

    eprintln!("Converted: {}", cli.input);
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
