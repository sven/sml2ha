use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

/// Configuration for the serial port.
#[derive(Debug, Deserialize)]
pub struct SerialConfig {
    /// The path to the serial device (e.g., /dev/ttyUSB0).
    pub port: String,
    /// The baud rate for the serial communication (usually 9600 for SML).
    pub baud_rate: u32,
}

/// Configuration for the MQTT broker.
#[derive(Debug, Deserialize)]
pub struct MqttConfig {
    /// The host address of the MQTT broker.
    pub host: String,
    /// The port of the MQTT broker.
    pub port: u16,
    /// The client ID to use for the MQTT connection.
    pub client_id: String,
    /// The base topic for MQTT messages.
    pub topic_prefix: String,
    /// Optional username for MQTT authentication.
    pub username: Option<String>,
    /// Optional password for MQTT authentication.
    pub password: Option<String>,
    /// Optional send cycle duration in seconds.
    pub send_cycle_sec: Option<u64>,
}

/// Configuration for logging and verbosity.
#[derive(Debug, Deserialize, Default)]
pub struct LoggingConfig {
    /// If true, prints telemetry data to the console every cycle.
    #[serde(default)]
    pub debug: bool,
}

/// Main application configuration, typically loaded from config.yml.
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    /// Serial port configuration.
    pub serial: SerialConfig,
    /// MQTT configuration.
    pub mqtt: MqttConfig,
    /// Logging configuration.
    pub logging: Option<LoggingConfig>,
}

impl AppConfig {
    /// Returns true if debug logging is enabled.
    pub fn debug(&self) -> bool {
        self.logging.as_ref().map(|l| l.debug).unwrap_or(false)
    }

    /// Loads the configuration from a YAML file at the specified path.
    pub fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let config_data =
            fs::read_to_string(path).context(format!("Failed to read config file {:?}", path))?;
        let config: AppConfig = serde_yaml::from_str(&config_data)
            .context(format!("Failed to parse config file {:?}", path))?;
        Ok(config)
    }
}
