use std::{collections::HashMap, hash::Hash, str::FromStr};

use darksouls3::sprj::{EventFlag, ItemId};
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer};

/// The slot data supplied by the Archipelago server which provides specific
/// information about how to set up this game.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotData {
    /// Event flags that must all be set to true in order for the player to be
    /// considered to have achieved their goal.
    #[serde(deserialize_with = "deserialize_goals")]
    pub goal: Vec<EventFlag>,

    /// A map from Archipelago's item IDs to DS3's.
    pub ap_ids_to_item_ids: HashMap<I64Key, DeserializableItemId>,

    /// A map from Archipelago's item IDs to the number of instances of that
    /// item the given ID should grant.
    pub item_counts: HashMap<I64Key, u32>,

    /// The options chosen by this player.
    pub options: Options,
}

/// Deserializes a list of event flags, defaulting to the flag for defeating
/// Soul of Cinder.
fn deserialize_goals<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<EventFlag>, D::Error> {
    if let Some(ids) = Option::<Vec<u32>>::deserialize(deserializer)? {
        ids.into_iter()
            .map(|i| {
                EventFlag::try_from(i).map_err(|_| {
                    D::Error::invalid_value(Unexpected::Unsigned(i.into()), &"a DS3 event flag")
                })
            })
            .collect()
    } else {
        // The DS3 AP 3.x world doesn't provide a list of goal events.
        Ok(vec![14100800.try_into().unwrap()])
    }
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

/// Deserializes an integer as a boolean value.
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

/// A deserializable wrapper over [ItemId].
#[derive(Debug, Deserialize)]
#[serde(try_from = "u32")]
#[repr(transparent)]
pub struct DeserializableItemId(pub ItemId);

impl TryFrom<u32> for DeserializableItemId {
    type Error = <ItemId as TryFrom<u32>>::Error;

    fn try_from(value: u32) -> Result<DeserializableItemId, Self::Error> {
        Ok(DeserializableItemId(value.try_into()?))
    }
}
