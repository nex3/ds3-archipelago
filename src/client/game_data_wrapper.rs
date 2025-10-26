use std::collections::HashMap;
use std::sync::Arc;

use archipelago_rs::protocol::*;

/// A wrapper of the Archipelago [GameData] object which provides a more
/// ergonomic API.
pub struct GameDataWrapper {
    id_to_item_name: HashMap<i64, Arc<String>>,
    id_to_location_name: HashMap<i64, Arc<String>>,
}

impl GameDataWrapper {
    pub fn new(inner: GameData) -> Self {
        GameDataWrapper {
            id_to_item_name: HashMap::from_iter(
                inner
                    .item_name_to_id
                    .into_iter()
                    .map(|(k, v)| (v, Arc::new(k))),
            ),
            id_to_location_name: HashMap::from_iter(
                inner
                    .location_name_to_id
                    .into_iter()
                    .map(|(k, v)| (v, Arc::new(k))),
            ),
        }
    }

    /// Returns the name for the item with the given ID. Panics if the ID isn't
    /// defined for this game.
    pub fn item_name(&self, id: i64) -> Arc<String> {
        self.id_to_item_name
            .get(&id)
            .expect("Expected item ID to have a name")
            .clone()
    }

    /// Returns the name for the location with the given ID. Panics if the ID isn't
    /// defined for this game.
    pub fn location_name(&self, id: i64) -> Option<Arc<String>> {
        if let Some(name) = self.id_to_location_name.get(&id) {
            Some(name.clone())
        } else {
            None
        }
    }
}
