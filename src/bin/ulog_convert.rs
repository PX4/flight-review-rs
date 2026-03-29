use std::path::Path;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);

    let input = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: ulog-convert <input.ulg> [output_dir]");
            std::process::exit(1);
        }
    };

    let output_dir = args
        .next()
        .unwrap_or_else(|| {
            let stem = Path::new(&input)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            format!("{}_parquet", stem)
        });

    let output_path = Path::new(&output_dir);

    let start = Instant::now();
    let result = match flight_review::converter::convert_ulog(&input, output_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    let elapsed = start.elapsed();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);
    let output_size: u64 = result
        .parquet_files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();

    println!("Converted: {}", input);
    println!("Output:    {} ({} files)", output_dir, result.parquet_files.len());
    println!(
        "Size:      {:.1} MB -> {:.1} MB ({:.0}% of original)",
        input_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / 1024.0 / 1024.0,
        output_size as f64 / input_size as f64 * 100.0
    );
    println!("Time:      {:.0}ms", elapsed.as_millis());
    println!("Throughput: {:.0} MB/s", input_size as f64 / 1024.0 / 1024.0 / elapsed.as_secs_f64());

    // Write metadata JSON
    let meta_path = output_path.join("metadata.json");
    let meta_json = serde_json::to_string_pretty(&result.metadata).unwrap();
    std::fs::write(&meta_path, &meta_json).unwrap();
    println!("Metadata:  {}", meta_path.display());

    if let Some(name) = &result.metadata.sys_name {
        println!("\nVehicle:   {} ({})",
            name,
            result.metadata.ver_hw.as_deref().unwrap_or("unknown hw"));
    }
    println!("Topics:    {}", result.metadata.topics.len());
    println!("Dropouts:  {} ({} ms total)", result.metadata.dropout_count, result.metadata.dropout_total_ms);
}
