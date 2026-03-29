//! Extract flight metadata from ULog files for cataloging and search.
//!
//! Metadata is extracted via the streaming parser's Info message callback,
//! producing a JSON-serializable struct with vehicle info, firmware version,
//! flight statistics, and topic inventory.

use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightMetadata {
    /// System name (e.g., "PX4")
    pub sys_name: Option<String>,
    /// Hardware version (e.g., "AUAV_X21", "Pixhawk 6C")
    pub ver_hw: Option<String>,
    /// Hardware subtype
    pub ver_hw_subtype: Option<String>,
    /// Software version git hash
    pub ver_sw: Option<String>,
    /// Software version release (encoded as 0xAABBCCTT)
    pub ver_sw_release: Option<u32>,
    /// Vehicle UUID
    pub sys_uuid: Option<String>,
    /// OS name
    pub sys_os_name: Option<String>,
    /// OS version
    pub sys_os_ver: Option<String>,
    /// Toolchain
    pub sys_toolchain: Option<String>,
    /// Toolchain version
    pub sys_toolchain_ver: Option<String>,
    /// UTC time reference offset in seconds
    pub time_ref_utc: Option<i64>,
    /// Topics found in the log with message counts
    pub topics: HashMap<String, TopicInfo>,
    /// Number of dropout events
    pub dropout_count: u32,
    /// Total dropout duration in milliseconds
    pub dropout_total_ms: u64,
    /// Number of logged string messages
    pub logged_message_count: u32,
    /// Number of parameters
    pub parameter_count: u32,
    /// Start timestamp from the file header (microseconds)
    pub start_timestamp_us: u64,
    /// All info key-value pairs (raw, for extensibility)
    pub info: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicInfo {
    pub message_count: usize,
    pub multi_id: u8,
}

impl Default for FlightMetadata {
    fn default() -> Self {
        Self {
            sys_name: None,
            ver_hw: None,
            ver_hw_subtype: None,
            ver_sw: None,
            ver_sw_release: None,
            sys_uuid: None,
            sys_os_name: None,
            sys_os_ver: None,
            sys_toolchain: None,
            sys_toolchain_ver: None,
            time_ref_utc: None,
            topics: HashMap::new(),
            dropout_count: 0,
            dropout_total_ms: 0,
            logged_message_count: 0,
            parameter_count: 0,
            start_timestamp_us: 0,
            info: HashMap::new(),
        }
    }
}

/// Extract metadata from a ULog file using the streaming parser.
pub fn extract_metadata(path: &str) -> Result<FlightMetadata, std::io::Error> {
    let mut meta = FlightMetadata::default();

    // Get header timestamp
    let data = std::fs::read(path)?;
    if data.len() >= 16 {
        let mut parser = px4_ulog::stream_parser::LogParser::default();
        parser.consume_bytes(&data[..16]).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}", e))
        })?;
        meta.start_timestamp_us = parser.timestamp();
    }

    // Stream through the file collecting metadata
    read_file_with_simple_callback(path, &mut |msg| {
        match msg {
            Message::Data(data) => {
                let topic = &data.flattened_format.message_name;
                let entry = meta.topics.entry(topic.clone()).or_insert(TopicInfo {
                    message_count: 0,
                    multi_id: data.multi_id.value(),
                });
                entry.message_count += 1;
            }
            Message::InfoMessage(info) => {
                let val_str = String::from_utf8_lossy(info.value).to_string();
                meta.info.insert(info.key.to_string(), val_str.clone());

                match info.key {
                    "sys_name" => meta.sys_name = Some(val_str),
                    "ver_hw" => meta.ver_hw = Some(val_str),
                    "ver_hw_subtype" => meta.ver_hw_subtype = Some(val_str),
                    "ver_sw" => meta.ver_sw = Some(val_str),
                    "sys_uuid" => meta.sys_uuid = Some(val_str),
                    "sys_os_name" => meta.sys_os_name = Some(val_str),
                    "sys_os_ver" => meta.sys_os_ver = Some(val_str),
                    "sys_toolchain" => meta.sys_toolchain = Some(val_str),
                    "sys_toolchain_ver" => meta.sys_toolchain_ver = Some(val_str),
                    "ver_sw_release" if info.value.len() == 4 => {
                        meta.ver_sw_release = Some(u32::from_le_bytes([
                            info.value[0],
                            info.value[1],
                            info.value[2],
                            info.value[3],
                        ]));
                    }
                    "time_ref_utc" if info.value.len() >= 4 => {
                        meta.time_ref_utc = Some(i32::from_le_bytes([
                            info.value[0],
                            info.value[1],
                            info.value[2],
                            info.value[3],
                        ]) as i64);
                    }
                    _ => {}
                }
            }
            Message::DropoutMessage(dropout) => {
                meta.dropout_count += 1;
                meta.dropout_total_ms += dropout.duration_ms as u64;
            }
            Message::LoggedMessage(_) => {
                meta.logged_message_count += 1;
            }
            Message::ParameterMessage(_) => {
                meta.parameter_count += 1;
            }
            _ => {}
        }
        SimpleCallbackResult::KeepReading
    })?;

    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px4_ulog_fixture(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest)
            .parent().unwrap()
            .join("px4-ulog-rs/tests/fixtures")
            .join(name);
        path.to_string_lossy().to_string()
    }

    #[test]
    fn test_extract_metadata_sample_ulg() {
        let meta = extract_metadata(&px4_ulog_fixture("sample.ulg")).unwrap();

        assert_eq!(meta.sys_name.as_deref(), Some("PX4"));
        assert_eq!(meta.ver_hw.as_deref(), Some("AUAV_X21"));
        assert_eq!(
            meta.ver_sw.as_deref(),
            Some("fd483321a5cf50ead91164356d15aa474643aa73")
        );
        assert_eq!(meta.start_timestamp_us, 112500176);
        assert_eq!(meta.dropout_count, 4);
        assert_eq!(meta.logged_message_count, 4);
        assert!(meta.parameter_count > 400);
        assert!(meta.topics.len() >= 15);
        assert!(meta.topics.contains_key("vehicle_attitude"));
        assert_eq!(meta.topics["vehicle_attitude"].message_count, 6461);
    }

    #[test]
    fn test_metadata_serializes_to_json() {
        let meta = extract_metadata(&px4_ulog_fixture("sample.ulg")).unwrap();
        let json = serde_json::to_string_pretty(&meta).unwrap();

        assert!(json.contains("PX4"));
        assert!(json.contains("AUAV_X21"));
        assert!(json.contains("vehicle_attitude"));
    }

    #[test]
    fn test_extract_metadata_fixed_wing() {
        let meta = extract_metadata(&px4_ulog_fixture("fixed_wing_gps.ulg")).unwrap();

        assert!(meta.sys_name.is_some());
        assert!(meta.topics.len() > 10);
    }
}
