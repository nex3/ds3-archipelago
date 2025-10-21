use std::collections::HashMap;

use darksouls3::sprj::CategorizedItemID;
use serde::{Deserialize, Deserializer, de};
use std::{fmt, hash::Hash};

/// The slot data supplied by the Archipelago server which provides specific
/// information about how to set up this game.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotData {
    /// A map from Archipelago's item IDs to DS3's.
    pub ap_ids_to_item_ids: HashMap<i64, DeserializableCategorizedItemID>,

    /// A map from Archipelago's item IDs to the number of instances of that
    /// item the given ID should grant.
    pub item_counts: HashMap<i64, u32>,

    /// The seed used to generate this multiworld. Together with the slot we
    /// consider this to uniquely identify a given save file.
    pub seed: String,

    /// The slot name for this player.
    pub slot: String,

    /// The options chosen by this player.
    pub options: Options,
}

#[derive(Debug, Deserialize)]
pub struct Options {
    /// Whether to kill the player when other players are killed and vice versa.
    pub death_link: bool,

    /// Whether the player's Archipelago expects the DS3 DLC to be enabled.
    pub enable_dlc: bool,
}

/// A deserializable wrapper over [CategorizedItemID].
#[derive(Debug, Deserialize)]
#[serde(try_from = "u32")]
#[repr(transparent)]
pub struct DeserializableCategorizedItemID(pub CategorizedItemID);

impl TryFrom<u32> for DeserializableCategorizedItemID {
    type Error = <CategorizedItemID as TryFrom<u32>>::Error;

    fn try_from(value: u32) -> Result<DeserializableCategorizedItemID, Self::Error> {
        Ok(DeserializableCategorizedItemID(value.try_into()?))
    }
}
