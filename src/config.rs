use std::fs;
use std::io;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths;

/// The configuration file for the DS3 Archipelago connection.
#[derive(Default, Deserialize, Serialize)]
pub struct Config {
    url: Option<String>,
    slot: Option<String>,
    password: Option<String>,
    version: Option<String>,
}

impl Config {
    /// Loads the config from disk, or returns an empty config if none exists on
    /// disk yet.
    pub fn load_or_default() -> Result<Self> {
        Self::load().map(|config| config.unwrap_or_default())
    }

    /// Loads the config from disk, or None if it doesn't exist.
    pub fn load() -> Result<Option<Self>> {
        match fs::read_to_string(Self::path()) {
            Ok(text) => Ok(Some(json::from_str(&text)?)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Saves the config file to disk.
    pub fn save(&self) -> Result<()> {
        Ok(fs::write(Self::path(), json::to_string(self)?)?)
    }

    /// The path to the configuration file.
    fn path() -> PathBuf {
        (*paths::MOD_DIRECTORY).join("apconfig.json")
    }

    /// Returns the Archipelago server URL defined in the config, or None if it
    /// doesn't contain a URL.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Sets the Archipelago server URL in the config file.
    pub fn set_url(&mut self, url: impl AsRef<str>) {
        self.url = Some(url.as_ref().to_string())
    }

    /// Returns the slot that the config was created with, or None if it
    /// doesn't contain a slot.
    pub fn slot(&self) -> Option<&str> {
        self.slot.as_deref()
    }

    /// Sets the Archipelago slot in the config file.
    pub fn set_slot(&mut self, slot: impl AsRef<str>) {
        self.slot = Some(slot.as_ref().to_string())
    }

    /// Returns the password that the config was created with, or None if it
    /// doesn't contain a password.
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    /// Sets the Archipelago password in the config file.
    pub fn set_password(&mut self, password: Option<impl AsRef<str>>) {
        self.password = password.map(|s| s.as_ref().to_string())
    }

    /// Returns the version that the config was created with, or None if it
    /// doesn't contain a version.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}
