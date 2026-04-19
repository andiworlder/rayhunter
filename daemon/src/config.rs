use log::warn;
use serde::{Deserialize, Serialize};

use rayhunter::Device;
use rayhunter::analysis::analyzer::AnalyzerConfig;

use crate::error::RayhunterError;
use crate::notifications::NotificationType;

/// Configuration for the BTS Observatory feature.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct BtsObservatoryConfig {
    /// Master enable switch for the BTS Observatory feature.
    pub enabled: bool,
    /// Size of the in-memory ring buffers for RSRP history and neighbor counts.
    pub live_ring_buffer_size: usize,
    /// How often to flush aggregated state to disk (seconds).
    pub flush_interval_seconds: u64,
    /// Max neighbors attached to a single Event.cell_context snapshot.
    pub max_neighbors_in_context: usize,
}

impl Default for BtsObservatoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            live_ring_buffer_size: 120,
            flush_interval_seconds: 10,
            max_neighbors_in_context: 8,
        }
    }
}

/// The structure of a valid rayhunter configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct Config {
    /// Path to store QMDL files
    pub qmdl_store_path: String,
    /// Listening port
    pub port: u16,
    /// Debug mode
    pub debug_mode: bool,
    /// Internal device name
    pub device: Device,
    /// UI level
    pub ui_level: u8,
    /// Colorblind mode
    pub colorblind_mode: bool,
    /// Key input mode
    pub key_input_mode: u8,
    /// ntfy.sh URL
    pub ntfy_url: Option<String>,
    /// Vector containing the types of enabled notifications
    pub enabled_notifications: Vec<NotificationType>,
    /// Vector containing the list of enabled analyzers
    pub analyzers: AnalyzerConfig,
    pub min_space_to_start_recording_mb: u64,
    pub min_space_to_continue_recording_mb: u64,
    #[serde(default)]
    pub bts_observatory: BtsObservatoryConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            qmdl_store_path: "/data/rayhunter/qmdl".to_string(),
            port: 8080,
            debug_mode: false,
            device: Device::Orbic,
            ui_level: 1,
            colorblind_mode: false,
            key_input_mode: 0,
            analyzers: AnalyzerConfig::default(),
            ntfy_url: None,
            enabled_notifications: vec![NotificationType::Warning, NotificationType::LowBattery],
            min_space_to_start_recording_mb: 1,
            min_space_to_continue_recording_mb: 1,
            bts_observatory: BtsObservatoryConfig::default(),
        }
    }
}

pub async fn parse_config<P>(path: P) -> Result<Config, RayhunterError>
where
    P: AsRef<std::path::Path>,
{
    if let Ok(config_file) = tokio::fs::read_to_string(&path).await {
        Ok(toml::from_str(&config_file).map_err(RayhunterError::ConfigFileParsingError)?)
    } else {
        warn!("unable to read config file, using default config");
        Ok(Config::default())
    }
}

pub struct Args {
    pub config_path: String,
}

pub fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} /path/to/config/file", args[0]);
        std::process::exit(1);
    }
    Args {
        config_path: args[1].clone(),
    }
}
