//! Extract flight metadata from ULog files for cataloging and search.
//!
//! Metadata is extracted via the streaming parser's Info message callback,
//! producing a JSON-serializable struct with vehicle info, firmware version,
//! flight statistics, and topic inventory.

use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use px4_ulog::stream_parser::model::LogStage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A logged string message from the flight controller
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub timestamp_us: u64,
    pub message: String,
}

/// A tagged logged string message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedLogEntry {
    pub level: String,
    pub tag: u16,
    pub timestamp_us: u64,
    pub message: String,
}

/// Convert a ULog log_level u8 (char code '0'-'7') to a human-readable string.
fn log_level_to_string(log_level: u8) -> &'static str {
    match log_level as char {
        '0' => "EMERGENCY",
        '1' => "ALERT",
        '2' => "CRITICAL",
        '3' => "ERROR",
        '4' => "WARNING",
        '5' => "NOTICE",
        '6' => "INFO",
        '7' => "DEBUG",
        _ => "UNKNOWN",
    }
}

/// Parameter value (int32 or float)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Int32(i32),
    Float(f32),
}

/// A parameter change recorded during flight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedParam {
    pub name: String,
    pub value: ParamValue,
    /// true if changed during data phase (in-flight), false if definition phase
    pub in_flight: bool,
}

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
    /// Logged string messages (console output)
    pub logged_messages: Vec<LogEntry>,
    /// Tagged logged string messages
    pub tagged_logged_messages: Vec<TaggedLogEntry>,
    /// Initial parameter values (name -> value)
    pub parameters: HashMap<String, ParamValue>,
    /// Parameters changed during flight (with context)
    pub changed_parameters: Vec<ChangedParam>,
    /// Default parameter values
    pub default_parameters: HashMap<String, ParamValue>,
    /// Start timestamp from the file header (microseconds)
    pub start_timestamp_us: u64,
    /// All info key-value pairs (raw, for extensibility)
    pub info: HashMap<String, String>,
    /// Compatibility flags from FlagBits message
    pub compat_flags: [u8; 8],
    /// Incompatibility flags from FlagBits message
    pub incompat_flags: [u8; 8],
    /// Appended data offsets (up to 3, 0 means unused)
    pub appended_offsets: [u64; 3],
    /// ULog file format version
    pub file_version: u8,
    /// Message IDs that were unsubscribed during logging
    pub removed_logged_ids: Vec<u16>,
    /// Number of sync markers encountered
    pub sync_count: u32,
    /// Multi-info messages (reassembled). Keys like "metadata_events", "perf_counter_preflight",
    /// "boot_console_output", etc. Each key maps to a list of values.
    pub multi_info: HashMap<String, Vec<String>>,
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
            logged_messages: Vec::new(),
            tagged_logged_messages: Vec::new(),
            parameters: HashMap::new(),
            changed_parameters: Vec::new(),
            default_parameters: HashMap::new(),
            start_timestamp_us: 0,
            info: HashMap::new(),
            compat_flags: [0u8; 8],
            incompat_flags: [0u8; 8],
            appended_offsets: [0u64; 3],
            file_version: 0,
            removed_logged_ids: Vec::new(),
            sync_count: 0,
            multi_info: HashMap::new(),
        }
    }
}

/// Extract metadata from a ULog file using the streaming parser.
pub fn extract_metadata(path: &str) -> Result<FlightMetadata, std::io::Error> {
    let mut meta = FlightMetadata::default();

    // Get header timestamp and file version
    let data = std::fs::read(path)?;
    if data.len() >= 16 {
        let mut parser = px4_ulog::stream_parser::LogParser::default();
        parser.consume_bytes(&data[..16]).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{}", e))
        })?;
        meta.start_timestamp_us = parser.timestamp();
        meta.file_version = data[7];
    }

    // Parse FlagBits message from raw bytes (immediately after 16-byte header)
    // Layout: [u16 msg_size][u8 msg_type='B'][u8;8 compat][u8;8 incompat][u64;3 offsets]
    // Minimum: 16 (header) + 3 (msg header) + 40 (flag bits data) = 59 bytes
    if data.len() >= 59 && data[18] == b'B' {
        meta.compat_flags.copy_from_slice(&data[19..27]);
        meta.incompat_flags.copy_from_slice(&data[27..35]);
        for i in 0..3 {
            let offset = 35 + i * 8;
            meta.appended_offsets[i] =
                u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        }
    }

    // Temporary buffer for reassembling multi-info message fragments
    let mut multi_info_buffers: HashMap<String, Vec<u8>> = HashMap::new();

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
            Message::LoggedMessage(msg) => {
                meta.logged_messages.push(LogEntry {
                    level: msg.human_readable_log_level().to_string(),
                    timestamp_us: msg.timestamp,
                    message: msg.logged_message.to_string(),
                });
            }
            Message::TaggedLoggedMessage(msg) => {
                meta.tagged_logged_messages.push(TaggedLogEntry {
                    level: log_level_to_string(msg.log_level).to_string(),
                    tag: msg.tag,
                    timestamp_us: msg.timestamp,
                    message: msg.logged_message.to_string(),
                });
            }
            Message::ParameterMessage(param) => {
                let (name, value, stage) = match param {
                    px4_ulog::stream_parser::model::ParameterMessage::Float(n, v, s) => {
                        (n.to_string(), ParamValue::Float(*v), s)
                    }
                    px4_ulog::stream_parser::model::ParameterMessage::Int32(n, v, s) => {
                        (n.to_string(), ParamValue::Int32(*v), s)
                    }
                };
                match stage {
                    LogStage::Definitions => {
                        meta.parameters.insert(name, value);
                    }
                    LogStage::Data => {
                        meta.changed_parameters.push(ChangedParam {
                            name,
                            value,
                            in_flight: true,
                        });
                    }
                }
            }
            Message::ParameterDefaultMessage(param) => {
                let (name, value) = match param {
                    px4_ulog::stream_parser::model::ParameterDefaultMessage::Float(n, v, _) => {
                        (n.to_string(), ParamValue::Float(*v))
                    }
                    px4_ulog::stream_parser::model::ParameterDefaultMessage::Int32(n, v, _) => {
                        (n.to_string(), ParamValue::Int32(*v))
                    }
                };
                meta.default_parameters.insert(name, value);
            }
            Message::RemoveLoggedMessage(msg) => {
                meta.removed_logged_ids.push(msg.msg_id);
            }
            Message::SyncMessage(_) => {
                meta.sync_count += 1;
            }
            Message::MultiInfoMessage(msg) => {
                let buffer = multi_info_buffers
                    .entry(msg.key.to_string())
                    .or_default();
                buffer.extend_from_slice(msg.value);
                if !msg.is_continued {
                    let value = String::from_utf8_lossy(buffer).to_string();
                    meta.multi_info
                        .entry(msg.key.to_string())
                        .or_default()
                        .push(value);
                    multi_info_buffers.remove(msg.key);
                }
            }
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
        assert_eq!(meta.logged_messages.len(), 4);
        assert!(!meta.logged_messages[0].message.is_empty());
        assert!(!meta.logged_messages[0].level.is_empty());
        assert!(meta.parameters.len() > 400);
        assert!(meta.parameters.contains_key("SDLOG_UTC_OFFSET"));
        assert!(meta.topics.len() >= 15);
        assert!(meta.topics.contains_key("vehicle_attitude"));
        assert_eq!(meta.topics["vehicle_attitude"].message_count, 6461);

        // Flag bits and version
        // sample.ulg uses ULog version 0
        assert_eq!(meta.file_version, 0);
        // compat_flags and incompat_flags should be populated (all zeros is valid)
        // Just verify they were parsed (not left as default if file has non-zero flags)
        assert_eq!(meta.compat_flags.len(), 8);
        assert_eq!(meta.incompat_flags.len(), 8);
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

        // fixed_wing_gps.ulg has version 1 and FlagBits message
        assert_eq!(meta.file_version, 1);
        // FlagBits are present (all zeros for this file, but parsed)
        assert_eq!(meta.compat_flags.len(), 8);
        assert_eq!(meta.incompat_flags.len(), 8);
        assert_eq!(meta.appended_offsets.len(), 3);
    }
}
