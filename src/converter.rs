//! Converts parsed ULog data to per-topic Parquet files.
//!
//! Each ULog topic (e.g., vehicle_attitude, sensor_combined) becomes a separate
//! Parquet file with columnar data. This is the core of the parse-once-store
//! architecture for Flight Review v2.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::*;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use px4_ulog::full_parser::SomeVec;

use crate::metadata::FlightMetadata;

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("ULog parse error: {0}")]
    Parse(#[from] std::io::Error),
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
    #[error("No data in ULog file")]
    NoData,
}

/// Result of converting a single ULog file.
pub struct ConvertResult {
    /// Paths to the generated Parquet files (one per topic+multi_id).
    pub parquet_files: Vec<PathBuf>,
    /// Extracted flight metadata.
    pub metadata: FlightMetadata,
}

/// Convert a ULog file to per-topic Parquet files in the given output directory.
pub fn convert_ulog(input_path: &str, output_dir: &Path) -> Result<ConvertResult, ConvertError> {
    std::fs::create_dir_all(output_dir).map_err(|e| ConvertError::Parse(e))?;

    // Parse the ULog file
    let parsed = px4_ulog::full_parser::read_file(input_path)?;

    // Extract metadata via the streaming parser
    let metadata = crate::metadata::extract_metadata(input_path)?;

    // Convert each topic to a Parquet file
    let mut parquet_files = Vec::new();

    for (topic_name, multi_map) in &parsed.messages {
        for (multi_id, fields) in multi_map {
            let filename = if multi_id.value() == 0 {
                format!("{}.parquet", topic_name)
            } else {
                format!("{}_{}.parquet", topic_name, multi_id.value())
            };
            let path = output_dir.join(&filename);

            write_topic_parquet(topic_name, fields, &path)?;
            parquet_files.push(path);
        }
    }

    Ok(ConvertResult {
        parquet_files,
        metadata,
    })
}

/// Write a single topic's data as a Parquet file.
fn write_topic_parquet(
    _topic_name: &str,
    fields: &HashMap<String, SomeVec>,
    output_path: &Path,
) -> Result<(), ConvertError> {
    if fields.is_empty() {
        return Ok(());
    }

    // Build Arrow schema and arrays from SomeVec data
    let mut arrow_fields = Vec::new();
    let mut arrow_arrays: Vec<ArrayRef> = Vec::new();

    // Put timestamp first if it exists
    let mut field_names: Vec<&String> = fields.keys().collect();
    field_names.sort();
    if let Some(pos) = field_names.iter().position(|n| n.as_str() == "timestamp") {
        let ts = field_names.remove(pos);
        field_names.insert(0, ts);
    }

    for name in &field_names {
        let vec = &fields[*name];
        let (field, array) = somevec_to_arrow(name, vec);
        arrow_fields.push(field);
        arrow_arrays.push(array);
    }

    let schema = Arc::new(Schema::new(arrow_fields));
    let batch = RecordBatch::try_new(schema.clone(), arrow_arrays)?;

    // Write Parquet with ZSTD compression
    let file = File::create(output_path)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(())
}

/// Convert a SomeVec to an Arrow Field + ArrayRef.
fn somevec_to_arrow(name: &str, vec: &SomeVec) -> (Field, ArrayRef) {
    match vec {
        SomeVec::Int8(v) => (
            Field::new(name, DataType::Int8, false),
            Arc::new(Int8Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::UInt8(v) => (
            Field::new(name, DataType::UInt8, false),
            Arc::new(UInt8Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Int16(v) => (
            Field::new(name, DataType::Int16, false),
            Arc::new(Int16Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::UInt16(v) => (
            Field::new(name, DataType::UInt16, false),
            Arc::new(UInt16Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Int32(v) => (
            Field::new(name, DataType::Int32, false),
            Arc::new(Int32Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::UInt32(v) => (
            Field::new(name, DataType::UInt32, false),
            Arc::new(UInt32Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Int64(v) => (
            Field::new(name, DataType::Int64, false),
            Arc::new(Int64Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::UInt64(v) => (
            Field::new(name, DataType::UInt64, false),
            Arc::new(UInt64Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Float(v) => (
            Field::new(name, DataType::Float32, false),
            Arc::new(Float32Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Double(v) => (
            Field::new(name, DataType::Float64, false),
            Arc::new(Float64Array::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Bool(v) => (
            Field::new(name, DataType::Boolean, false),
            Arc::new(BooleanArray::from(v.clone())) as ArrayRef,
        ),
        SomeVec::Char(v) => {
            let strings: Vec<String> = v.iter().map(|c| c.to_string()).collect();
            (
                Field::new(name, DataType::Utf8, false),
                Arc::new(StringArray::from(
                    strings.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )) as ArrayRef,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px4_ulog_fixture(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest)
            .parent().unwrap()  // ulog/
            .join("px4-ulog-rs/tests/fixtures")
            .join(name);
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_convert_sample_ulg() {
        let tmp = tempfile::tempdir().unwrap();
        let result = convert_ulog(&px4_ulog_fixture("sample.ulg"), tmp.path()).unwrap();

        assert!(!result.parquet_files.is_empty(), "Should produce Parquet files");

        // Should have one file per topic
        let has_attitude = result
            .parquet_files
            .iter()
            .any(|p| p.file_name().unwrap().to_str().unwrap() == "vehicle_attitude.parquet");
        assert!(has_attitude, "Should produce vehicle_attitude.parquet");
    }

    #[test]
    fn test_parquet_file_readable() {
        let tmp = tempfile::tempdir().unwrap();
        let result = convert_ulog(&px4_ulog_fixture("sample.ulg"), tmp.path()).unwrap();

        // Read back a Parquet file and verify row count
        let attitude_path = result
            .parquet_files
            .iter()
            .find(|p| p.file_name().unwrap().to_str().unwrap() == "vehicle_attitude.parquet")
            .expect("vehicle_attitude.parquet should exist");

        let file = File::open(attitude_path).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();

        let mut total_rows = 0;
        for batch in reader {
            total_rows += batch.unwrap().num_rows();
        }
        // pyulog reports 6461 vehicle_attitude messages in sample.ulg
        assert_eq!(total_rows, 6461, "vehicle_attitude should have 6461 rows");
    }

    #[test]
    fn test_parquet_has_timestamp_first() {
        let tmp = tempfile::tempdir().unwrap();
        convert_ulog(&px4_ulog_fixture("sample.ulg"), tmp.path()).unwrap();

        let path = tmp.path().join("sensor_combined.parquet");
        let file = File::open(&path).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
            .unwrap()
            .build()
            .unwrap();

        let batch = reader.into_iter().next().unwrap().unwrap();
        assert_eq!(
            batch.schema().field(0).name(),
            "timestamp",
            "First column should be timestamp"
        );
    }

    #[test]
    fn test_convert_fixed_wing() {
        let tmp = tempfile::tempdir().unwrap();
        let result =
            convert_ulog(&px4_ulog_fixture("fixed_wing_gps.ulg"), tmp.path()).unwrap();

        assert!(
            result.parquet_files.len() > 10,
            "Fixed-wing log should produce many Parquet files, got {}",
            result.parquet_files.len()
        );
    }
}
