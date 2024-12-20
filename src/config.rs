/*
 * A Configuration and Control UI for ODR-DabMux
 * Copyright (C) 2024 Matthias P. Braendli
 *
 * This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public
 * License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied
 * warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use std::{collections::HashMap, fs};
use anyhow::Context;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::json;

type Protection = u8;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Service {
    pub unique_id: String,
    pub sid: u32,
    pub ecc: u8,
    pub label: String,
    pub shortlabel: String,
    pub input_port: u16,
    pub bitrate: u32,
    pub protection: Protection,
}

impl Service {
    pub fn sid_hex(&self) -> String {
        format!("{:04X}", self.sid)
    }

    pub fn ecc_hex(&self) -> String {
        format!("{:02X}", self.ecc)
    }

    pub fn dump_to_service_json(&self) -> serde_json::Value {
        json!({
            "id": self.sid,
            "ecc": self.ecc,
            "label": self.label,
            "shortlabel": self.shortlabel,
        })
    }

    pub fn dump_to_subchannel_json(&self, id: u32) -> serde_json::Value {
        json!({
            "type": "dabplus",
            "bitrate": self.bitrate,
            "id": id,
            "protection": self.protection,

            "inputproto": "edi",
            "inputuri": format!("tcp://127.0.0.1:{}", self.input_port),
            "buffer-management": "prebuffering",
            "buffer": 40,
            "prebuffering": 20
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub instance_name: String,
    pub dabmux_config_location: String,
    pub tist: bool,
    pub tist_offset: i32,
    // TODO tai_clock_bulletins
    pub ensemble_id: u16,
    pub ensemble_ecc: u8,
    pub ensemble_label: String,
    pub ensemble_shortlabel: String,
    pub output_edi_port: u16,
    pub output_zmq_port: u16,
    pub services: Vec<Service>,
}

impl Config {
    pub fn ensemble_id_hex(&self) -> String {
        format!("{:04X}", self.ensemble_id)
    }

    pub fn ensemble_ecc_hex(&self) -> String {
        format!("{:02X}", self.ensemble_ecc)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            instance_name: "CHANGEME".to_owned(),
            dabmux_config_location: "/etc/odr-dabmux.json".to_owned(),
            tist: true,
            tist_offset: 0,
            ensemble_id: 0x4FFF,
            ensemble_ecc: 0xE1,
            ensemble_label: "OpenDigitalRadio".to_owned(),
            ensemble_shortlabel: "ODR".to_owned(),
            output_edi_port: 8951,
            output_zmq_port: 8851,
            services: vec![
               Service {
                   unique_id: "nothing".to_owned(),
                   sid: 0x4DAA,
                   ecc: 0xE1,
                   label: "nothing".to_owned(),
                   shortlabel: "no".to_owned(),
                   input_port: 9001,
                   bitrate: 128,
                   protection: 2
               }
            ],
        }
    }
}

const CONFIGFILE : &str = "odr-dabmux-gui-config.toml";

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        if std::path::Path::new(CONFIGFILE).exists() {
            let file_contents = fs::read_to_string(CONFIGFILE)?;
            toml::from_str(&file_contents)
                .or_else(|e| {
                    error!("Failed to read existing config file: {}", e);
                    Ok(Default::default())
                })
        }
        else {
            Ok(Default::default())
        }
    }

    pub fn store(&self) -> anyhow::Result<()> {
        fs::write(CONFIGFILE, toml::to_string_pretty(&self)?)
            .context("writing config file")
    }

    pub fn write_dabmux_json(&self) -> anyhow::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        let mut services = HashMap::new();
        for s in &self.services {
            let uid = format!("srv-{}", s.unique_id);
            services.insert(uid, s.dump_to_service_json());
        }

        let mut subchannels = HashMap::new();
        let mut id = 0;
        for s in &self.services {
            id += 1;
            let uid = format!("sub-{}", s.unique_id);
            subchannels.insert(uid, s.dump_to_subchannel_json(id));
        }

        let mut components = HashMap::new();
        for s in &self.services {
            components.insert(
                format!("comp-{}", s.unique_id),
                json!({
                    "service": format!("srv-{}", s.unique_id),
                    "subchannel": format!("sub-{}", s.unique_id),
                    "user-applications": {
                        "userapp": "slideshow"
                    }
                }));
        }

        let new_conf = json!({
            "_comment": format!("Generated at {} by odr-dabmux-gui", now),
            "general": {
                "dabmode": 1,
                "nbframes": 0,
                "syslog": false,
                "tist": self.tist,
                "tist_offset": self.tist_offset,
                "managementport": 12720
            },
            "remotecontrol": {
                "telnetport": 12721,
                "zmqendpoint": "tcp://lo:12722"
            },
            "ensemble": {
                "id": self.ensemble_id,
                "ecc": self.ensemble_ecc,
                "local-time-offset": "auto",
                "reconfig-counter": "hash",
                "label": self.ensemble_label,
                "shortlabel": self.ensemble_shortlabel
            },
            "services": services,
            "subchannels": subchannels,
            "components": components,
            "outputs": {
                "throttle": "simul://",
                "zeromq": {
                    "endpoint": format!("tcp://*:{}", self.output_zmq_port),
                    "allowmetadata": false
                },
                "edi": {
                    "destinations": {
                        "example_tcp": {
                            "protocol": "tcp",
                            "listenport": self.output_edi_port
                        }
                    }
                }
            }
        });

        fs::write(&self.dabmux_config_location, serde_json::to_string_pretty(&new_conf)?)
            .context("writing dabmux config file")
    }
}
