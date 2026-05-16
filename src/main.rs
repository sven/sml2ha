//! SML to MQTT Bridge
//!
//! This application reads SML (Smart Message Language) data from a smart meter
//! and publishes it to Home Assistant via MQTT.

use anyhow::Result;
use clap::Parser;

mod config;
mod ha;
mod sml_conn;

use config::AppConfig;
use ha::{HomeAssistant, TelemetryResult};
use sml_conn::SmlConn;

/// Command line arguments for sml2ha.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.yml")]
    config: std::path::PathBuf,
}

fn main() -> Result<()> {
    // 1. Parse command line arguments
    let args = Args::parse();

    // 2. Load application configuration
    let config = AppConfig::load(&args.config)?;

    println!("Using configuration from: {}", args.config.display());
    println!(
        "Using serial port: {} at {} baud",
        config.serial.port, config.serial.baud_rate
    );

    // 3. Initialize SML connection
    let mut sml = SmlConn::new(&config.serial.port, config.serial.baud_rate)?;

    // 4. Initialize Home Assistant MQTT client (deferred until first SML packet)
    let mut ha: Option<HomeAssistant> = None;

    // 5. Main loop
    println!("Starting SML telemetry loop. Press Ctrl+C to stop.");
    let mut registered = false;
    let mut last_alive_print = chrono::Local::now();

    loop {
        match sml.read_telemetry() {
            Ok(data) => {
                if ha.is_none() {
                    let device_id = format!("sml_{}", data.serial);
                    ha = Some(HomeAssistant::new(
                        &config.mqtt,
                        &device_id,
                        &data.manufacturer,
                        &data.model,
                    )?);
                    println!(
                        "Initialized Home Assistant: {} {} (Serial: {})",
                        data.manufacturer, data.model, data.serial
                    );
                }

                let ha_client = ha.as_mut().unwrap();

                let results = vec![
                    TelemetryResult {
                        name: "Energy Import Total".to_string(),
                        value: data.energy_import_total,
                        unit: "kWh".to_string(),
                        device_class: Some("energy".to_string()),
                        state_class: Some("total_increasing".to_string()),
                    },
                    TelemetryResult {
                        name: "Energy Export Total".to_string(),
                        value: data.energy_export_total,
                        unit: "kWh".to_string(),
                        device_class: Some("energy".to_string()),
                        state_class: Some("total_increasing".to_string()),
                    },
                    TelemetryResult {
                        name: "Power Current Total".to_string(),
                        value: data.power_total,
                        unit: "W".to_string(),
                        device_class: Some("power".to_string()),
                        state_class: Some("measurement".to_string()),
                    },
                ];

                if !registered {
                    println!("Registering sensors with Home Assistant...");
                    if let Err(e) = ha_client.register_sensors(&results) {
                        eprintln!("Failed to register sensors: {}", e);
                    } else {
                        registered = true;
                    }
                }

                if config.debug() {
                    println!(
                        "--- SML Telemetry ({}) ---",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
                    );
                    for res in &results {
                        println!("{:<20}: {:>10.2} {}", res.name, res.value, res.unit);
                    }
                    println!("--------------------------\n");
                }

                if let Err(e) = ha_client.publish_state(&results) {
                    eprintln!("Failed to publish MQTT state: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to read SML telemetry: {}", e);
            }
        }

        if !config.debug() {
            let now = chrono::Local::now();
            if now.signed_duration_since(last_alive_print).num_hours() >= 1 {
                println!("alive ({})", now.format("%Y-%m-%d %H:%M:%S"));
                last_alive_print = now;
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
