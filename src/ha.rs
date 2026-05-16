//! Home Assistant MQTT Integration
//!
//! This module provides the `HomeAssistant` struct which handles the registration
//! and update of telemetry sensors in Home Assistant using its MQTT Discovery
//! protocol.

use crate::config::MqttConfig;
use anyhow::Result;
use rumqttc::{Client, MqttOptions, QoS};
use serde::Serialize;
use serde_json::json;

/// Represents a successfully retrieved telemetry value.
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryResult {
    /// Human-readable channel name.
    pub name: String,
    /// Processed numeric value.
    pub value: f64,
    /// Measurement unit.
    pub unit: String,
    /// Device class (e.g., energy, power)
    pub device_class: Option<String>,
    /// State class (e.g., total_increasing, measurement)
    pub state_class: Option<String>,
}

/// Handles integration with Home Assistant via MQTT Discovery (autodetection).
pub struct HomeAssistant {
    client: Client,
    topic_prefix: String,
    device_id: String,
    device_name: String,
    manufacturer: String,
    model: String,
}

impl HomeAssistant {
    pub fn new(
        config: &MqttConfig,
        device_id: &str,
        manufacturer: &str,
        model: &str,
    ) -> Result<Self> {
        let mut mqttoptions = MqttOptions::new(&config.client_id, &config.host, config.port);
        mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

        if let (Some(u), Some(p)) = (&config.username, &config.password) {
            mqttoptions.set_credentials(u, p);
        }

        let (client, mut connection) = Client::new(mqttoptions, 10);

        std::thread::spawn(move || {
            for notification in connection.iter() {
                match notification {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("MQTT connection error: {:?}", e);
                        std::thread::sleep(std::time::Duration::from_secs(5));
                    }
                }
            }
        });

        let mut ha = Self {
            client,
            topic_prefix: config.topic_prefix.clone(),
            device_id: device_id.to_string(),
            device_name: format!("{} {}", manufacturer, model),
            manufacturer: manufacturer.to_string(),
            model: model.to_string(),
        };
        ha.device_id = ha.slugify(&ha.device_id);
        Ok(ha)
    }

    pub fn register_sensors(&mut self, sensors: &[TelemetryResult]) -> Result<()> {
        for sensor in sensors {
            let sensor_id = self.slugify(&sensor.name);
            let discovery_id = format!("{}_{}", self.device_id, sensor_id);
            let discovery_topic = format!("homeassistant/sensor/{}/config", discovery_id);

            let mut payload = json!({
                "name": format!("{}", sensor.name),
                "object_id": discovery_id,
                "state_topic": format!("{}/state", self.topic_prefix),
                "unit_of_measurement": sensor.unit,
                "value_template": format!("{{{{ value_json['{}'] }}}}", sensor_id),
                "unique_id": discovery_id,
                "device": {
                    "identifiers": [self.device_id],
                    "name": self.device_name,
                    "manufacturer": self.manufacturer,
                    "model": self.model
                }
            });

            if let Some(dc) = &sensor.device_class {
                payload["device_class"] = json!(dc);
            }
            if let Some(sc) = &sensor.state_class {
                payload["state_class"] = json!(sc);
            }

            self.client
                .publish(discovery_topic, QoS::AtLeastOnce, true, payload.to_string())?;
        }
        Ok(())
    }

    pub fn publish_state(&mut self, results: &[TelemetryResult]) -> Result<()> {
        let mut state = serde_json::Map::new();
        for res in results {
            state.insert(self.slugify(&res.name), json!(res.value));
        }

        let payload = serde_json::Value::Object(state).to_string();
        let topic = format!("{}/state", self.topic_prefix);

        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)?;
        Ok(())
    }

    fn slugify(&self, name: &str) -> String {
        name.replace([' ', '.', '-'], "_").to_lowercase()
    }
}
