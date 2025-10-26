use std::sync::Arc;

use archipelago_rs::protocol::*;
use darksouls3::sprj::CategorizedItemID;

/// All information about a Dark Souls III item provided by Archipelago.
pub struct Item {
    ap: NetworkItem,
    name: Arc<String>,
    location_name: Option<Arc<String>>,
    ds3_id: CategorizedItemID,
    quantity: u32,
}

impl Item {
    pub(super) fn new(
        ap: NetworkItem,
        name: Arc<String>,
        location_name: Option<Arc<String>>,
        ds3_id: CategorizedItemID,
        quantity: u32,
    ) -> Self {
        Item {
            ap,
            name,
            location_name,
            ds3_id,
            quantity,
        }
    }

    /// Returns the Archipelago ID for this item.
    pub fn ap_id(&self) -> i64 {
        self.ap.item
    }

    /// Returns the DS3 ID for this item.
    pub fn ds3_id(&self) -> CategorizedItemID {
        self.ds3_id
    }

    /// Returns the number of instances of this item that should be granted to
    /// the user.
    pub fn quantity(&self) -> u32 {
        self.quantity
    }

    /// Returns the Archipelago location ID for this item.
    pub fn ap_location_id(&self) -> i64 {
        self.ap.location
    }

    /// Returns whether this item can unlock logical advancement.
    pub fn is_progression(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::PROGRESSION)
    }

    /// Returns whether this item is especially useful.
    pub fn is_useful(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::USEFUL)
    }

    /// Returns whether this item is a trap.
    pub fn is_trap(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::TRAP)
    }

    /// Returns Archipelago's name for this item.
    pub fn ap_name(&self) -> &str {
        self.name.as_ref()
    }

    /// Returns Archipelago's name for this item's location, or None if the item
    /// has no location (such as a starting inventory item).
    pub fn location_name(&self) -> Option<&str> {
        if let Some(name) = &self.location_name {
            Some(name.as_ref())
        } else {
            None
        }
    }
}
