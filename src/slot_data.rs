use std::collections::HashMap;

use darksouls3::sprj::CategorizedItemID;
use serde::{Deserialize, Deserializer};
use std::{hash::Hash, str::FromStr};

/// The slot data supplied by the Archipelago server which provides specific
/// information about how to set up this game.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotData {
    /// A map from Archipelago's item IDs to DS3's.
    pub ap_ids_to_item_ids: HashMap<I64Key, DeserializableCategorizedItemID>,

    /// A map from Archipelago's item IDs to the number of instances of that
    /// item the given ID should grant.
    pub item_counts: HashMap<I64Key, u32>,

    /// The options chosen by this player.
    pub options: Options,
}

#[derive(Debug, Deserialize)]
pub struct Options {
    /// Whether to kill the player when other players are killed and vice versa.
    #[serde(deserialize_with = "int_to_bool")]
    pub death_link: bool,

    /// Whether the player's Archipelago expects the DS3 DLC to be enabled.
    #[serde(deserialize_with = "int_to_bool")]
    pub enable_dlc: bool,
}

/// Deserialized an integer as a boolean value.
fn int_to_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(u64::deserialize(deserializer)? != 0)
}

#[derive(Debug, Deserialize, Hash, PartialEq, Eq)]
#[serde(try_from = "&str")]
#[repr(transparent)]
pub struct I64Key(pub i64);

impl TryFrom<&str> for I64Key {
    type Error = <i64 as FromStr>::Err;

    fn try_from(value: &str) -> Result<I64Key, Self::Error> {
        Ok(I64Key(i64::from_str(value)?))
    }
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
