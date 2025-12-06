use std::sync::Arc;

use archipelago_rs::protocol::*;
use darksouls3::sprj::CategorizedItemID;

/// All information about a Dark Souls III item provided by Archipelago.
pub struct Item {
    ap: NetworkItem,
    ap_name: Arc<String>,
    location_name: Option<Arc<String>>,
    ds3_id: CategorizedItemID,
    quantity: u32,
    index: u64,
}

impl Item {
    pub(super) fn new(
        ap: NetworkItem,
        ap_name: Arc<String>,
        location_name: Option<Arc<String>>,
        ds3_id: CategorizedItemID,
        quantity: u32,
        index: u64,
    ) -> Self {
        Item {
            ap,
            ap_name,
            location_name,
            ds3_id,
            quantity,
            index,
        }
    }

    /// The Archipelago ID for this item.
    pub fn ap_id(&self) -> i64 {
        self.ap.item
    }

    /// The DS3 ID for this item.
    pub fn ds3_id(&self) -> CategorizedItemID {
        self.ds3_id
    }

    /// The number of instances of this item that should be granted to the user.
    pub fn quantity(&self) -> u32 {
        self.quantity
    }

    /// The Archipelago location ID for this item.
    #[allow(dead_code)]
    pub fn ap_location_id(&self) -> i64 {
        self.ap.location
    }

    /// Whether this item can unlock logical advancement.
    #[allow(dead_code)]
    pub fn is_progression(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::PROGRESSION)
    }

    /// Whether this item is especially useful.
    #[allow(dead_code)]
    pub fn is_useful(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::USEFUL)
    }

    /// Whether this item is a trap.
    #[allow(dead_code)]
    pub fn is_trap(&self) -> bool {
        self.ap.flags.contains(NetworkItemFlags::TRAP)
    }

    /// Archipelago's name for this item.
    #[allow(dead_code)]
    pub fn ap_name(&self) -> &str {
        self.ap_name.as_ref()
    }

    /// Archipelago's name for this item's location, or None if the item has no
    /// location (such as a starting inventory item).
    #[allow(dead_code)]
    pub fn location_name(&self) -> Option<&str> {
        if let Some(name) = &self.location_name {
            Some(name.as_ref())
        } else {
            None
        }
    }

    /// The absolute index of this item among all items received from the game
    /// at any point.
    pub fn index(&self) -> u64 {
        self.index
    }
}
