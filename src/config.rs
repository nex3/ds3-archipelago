use std::fs;
use std::io;
use std::path::PathBuf;
use std::result::Result;

use json::Value;

use crate::paths;

/// The configuration file for the DS3 Archipelago connection.
#[derive(Default)]
pub struct Config {
    json: json::Map<String, Value>,
}

impl Config {
    /// Loads the config from disk, or returns an empty config if none exists on
    /// disk yet.
    pub fn load_or_default() -> Result<Self, String> {
        Self::load().map(|config| config.unwrap_or_default())
    }

    /// Loads the config from disk, or None if it doesn't exist.
    pub fn load() -> Result<Option<Self>, String> {
        match fs::read_to_string(Self::path()) {
            Ok(text) => Ok(Some(Config {
                json: json::from_str(&text).map_err(|e| e.to_string())?,
            })),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    /// Saves the config file to disk.
    pub fn save(&self) -> Result<(), String> {
        json::to_string(&self.json)
            .map_err(|e| e.to_string())
            .and_then(|json| fs::write(Self::path(), json).map_err(|e| e.to_string()))
    }

    fn path() -> PathBuf {
        (*paths::MOD_DIRECTORY).join("apconfig.json")
    }

    /// Returns the Archipelago server URL defined in the config, or None if it
    /// doesn't contain a URL.
    pub fn url(&self) -> Option<&String> {
        if let Some(Value::String(url)) = self.json.get("url") {
            Some(url)
        } else {
            None
        }
    }

    /// Sets the Archipelago server URL in the config file.
    pub fn set_url(&mut self, url: impl AsRef<String>) {
        self.json
            .insert("url".to_string(), Value::String(url.as_ref().clone()));
    }

    /// Returns the slot that the config was created with, or None if it
    /// doesn't contain a slot.
    pub fn slot(&self) -> Option<&String> {
        if let Some(Value::String(slot)) = self.json.get("slot") {
            Some(slot)
        } else {
            None
        }
    }

    /// Sets the Archipelago slot in the config file.
    pub fn set_slot(&mut self, slot: impl AsRef<String>) {
        self.json
            .insert("slot".to_string(), Value::String(slot.as_ref().clone()));
    }

    /// Returns the password that the config was created with, or None if it
    /// doesn't contain a password.
    pub fn password(&self) -> Option<&String> {
        if let Some(Value::String(password)) = self.json.get("password") {
            Some(password)
        } else {
            None
        }
    }

    /// Sets the Archipelago password in the config file.
    pub fn set_password(&mut self, password: impl AsRef<String>) {
        self.json.insert(
            "password".to_string(),
            Value::String(password.as_ref().clone()),
        );
    }

    /// Returns the version that the config was created with, or None if it
    /// doesn't contain a version.
    pub fn version(&self) -> Option<&String> {
        if let Some(Value::String(version)) = self.json.get("version") {
            Some(version)
        } else {
            None
        }
    }
}
