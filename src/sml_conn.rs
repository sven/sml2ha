use anyhow::{Context, Result};
use serialport::SerialPort;
use sml_rs::ReadParsedError;
use sml_rs::SmlReader;
use sml_rs::parser::complete::File;
use sml_rs::util::{ArrayBuf, IoReader};
use std::time::Duration;

pub struct SmlConn {
    reader: SmlReader<IoReader<Box<dyn SerialPort>>, ArrayBuf<8192>>,
}

#[derive(Debug, Clone, Default)]
pub struct SmlTelemetry {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub energy_import_total: f64, // 1.8.0 (kWh or Wh)
    pub energy_export_total: f64, // 2.8.0 (kWh or Wh)
    pub power_total: f64,         // 16.7.0 (W)
}

impl SmlConn {
    pub fn new(port_path: &str, baud_rate: u32) -> Result<Self> {
        let port = serialport::new(port_path, baud_rate)
            .timeout(Duration::from_millis(5000))
            .open()
            .context(format!("Failed to open serial port {}", port_path))?;

        let reader = SmlReader::from_reader(port);
        Ok(Self { reader })
    }

    pub fn read_telemetry(&mut self) -> Result<SmlTelemetry> {
        loop {
            match self.reader.read::<File>() {
                Ok(sml_file) => {
                    return Ok(process_file(&sml_file));
                }
                Err(ReadParsedError::IoErr(e, _)) => {
                    return Err(anyhow::anyhow!("IO Error reading SML: {:?}", e));
                }
                Err(e) => {
                    eprintln!("SML read error: {:?}", e);
                }
            }
        }
    }
}

fn process_file(sml_file: &File) -> SmlTelemetry {
    let mut telemetry = SmlTelemetry::default();

    for message in &sml_file.messages {
        use sml_rs::parser::complete::MessageBody;

        if let MessageBody::GetListResponse(body) = &message.message_body {
            telemetry.serial = bytes_to_string(&body.server_id);

            // Try to decode manufacturer and model from server_id
            let (m, mdl) = decode_server_id(&body.server_id);
            if let Some(m) = m {
                telemetry.manufacturer = m;
            }
            if let Some(mdl) = mdl {
                telemetry.model = mdl;
            }

            for entry in &body.val_list {
                let obis = obis_to_string(&entry.obj_name);

                match obis.as_str() {
                    "0100010800FF" | "0100010800" => {
                        let val = extract_value(entry);
                        // If unit is Wh (30), convert to kWh
                        telemetry.energy_import_total = if entry.unit == Some(30) {
                            val / 1000.0
                        } else {
                            val
                        };
                    }
                    "0100020800FF" | "0100020800" => {
                        let val = extract_value(entry);
                        // If unit is Wh (30), convert to kWh
                        telemetry.energy_export_total = if entry.unit == Some(30) {
                            val / 1000.0
                        } else {
                            val
                        };
                    }
                    "0100100700FF" | "0100100700" => telemetry.power_total = extract_value(entry),
                    "8181C78203FF" => {
                        let m = extract_string(entry);
                        if !m.is_empty() {
                            telemetry.manufacturer = m;
                        }
                    }
                    "0100000001FF" => {
                        let m = extract_string(entry);
                        if !m.is_empty() {
                            telemetry.model = m;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Fallback if manufacturer/model not found in OBIS list
    if telemetry.manufacturer.is_empty() {
        telemetry.manufacturer = "SML Meter".to_string();
    }
    if telemetry.model.is_empty() {
        telemetry.model = "Generic".to_string();
    }

    telemetry
}

fn obis_to_string(obj_name: &[u8]) -> String {
    obj_name.iter().map(|b| format!("{:02X}", b)).collect()
}

fn extract_value(entry: &sml_rs::parser::common::ListEntry) -> f64 {
    use sml_rs::parser::common::Value;

    let raw_val = match &entry.value {
        Value::U8(v) => *v as f64,
        Value::U16(v) => *v as f64,
        Value::U32(v) => *v as f64,
        Value::U64(v) => *v as f64,
        Value::I8(v) => *v as f64,
        Value::I16(v) => *v as f64,
        Value::I32(v) => *v as f64,
        Value::I64(v) => *v as f64,
        _ => 0.0,
    };

    if let Some(scaler) = entry.scaler {
        raw_val * 10.0f64.powi(scaler as i32)
    } else {
        raw_val
    }
}

fn extract_string(entry: &sml_rs::parser::common::ListEntry) -> String {
    use sml_rs::parser::common::Value;

    match &entry.value {
        Value::Bytes(bytes) => bytes_to_string(bytes),
        _ => "".to_string(),
    }
}

fn bytes_to_string(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        let s = s.trim();
        if !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
        {
            return s.to_string();
        }
    }
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

fn decode_server_id(server_id: &[u8]) -> (Option<String>, Option<String>) {
    // FNN Server ID usually has a 3-letter manufacturer code in ASCII.
    // It can start at index 1 (standard) or index 2 (e.g. EasyMeter).
    let (m_offset, m_bytes) =
        if server_id.len() >= 5 && server_id[1..4].iter().all(|&b| b.is_ascii_alphabetic()) {
            (1, &server_id[1..4])
        } else if server_id.len() >= 6 && server_id[2..5].iter().all(|&b| b.is_ascii_alphabetic()) {
            (2, &server_id[2..5])
        } else {
            return (None, None);
        };

    let manufacturer = String::from_utf8_lossy(m_bytes).into_owned();
    let model_byte = server_id[m_offset + 3];

    let model = match manufacturer.as_str() {
        "EBZ" => match model_byte {
            0x01 => Some("DD3".to_string()),
            0x02 => Some("ED3".to_string()),
            _ => None,
        },
        "EMH" => {
            let next_offset = m_offset + 4;
            if server_id.len() > next_offset {
                let next_byte = server_id[next_offset];
                if (model_byte == 0x00 && next_byte == 0x01)
                    || (model_byte == 0x01 && next_byte == 0x01)
                {
                    Some("mMe".to_string())
                } else {
                    None
                }
            } else {
                None
            }
        }
        "ESY" => match model_byte {
            0x11 => Some("Q3A".to_string()),
            _ => Some("Meter".to_string()),
        },
        "APA" => Some("NORAX/APOX+".to_string()),
        "LGX" | "LMN" => Some("LK13".to_string()),
        _ => None,
    };

    (Some(manufacturer), model)
}
