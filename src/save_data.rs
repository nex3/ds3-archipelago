use std::collections::HashSet;
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use bincode::{Decode, Encode};
use darksouls3::sprj::MapItemMan;
use darksouls3::util::save;
use fromsoftware_shared::FromStatic;
use log::*;

/// The singleton instance of the save data, or None if it hasn't been loaded
/// from the save file or set explicitly.
static INSTANCE: LazyLock<RwLock<SaveData>> = LazyLock::new(|| {
    RwLock::new(SaveData {
        items_granted: Default::default(),
        locations: Default::default(),
        seed: None,
    })
});

/// The configuration for the binary encoding of the save data.
const CONFIG: bincode::config::Configuration = bincode::config::standard();

/// Data that's saved and loaded along with the player's game save.
#[derive(Debug, Decode, Encode)]
pub struct SaveData {
    /// The set of all Archipelago item IDs that have been granted to this
    /// player from foreign games throughout the course of this run.
    pub items_granted: HashSet<i64>,

    /// The set of Archipelago locations that this player has accessed so far in
    /// this game.
    pub locations: HashSet<i64>,

    /// The Archipelago seed this save file was last connected to. This is used
    /// to verify that the player doesn't accidentally corrupt a save by loading
    /// into it while connected to the wrong multiworld.
    pub seed: Option<String>,
}

impl SaveData {
    /// Register hooks for loading and unloading saves. These hooks are never
    /// unregistered.
    ///
    /// Safety: Follow all ilhook safety guidelines.
    pub unsafe fn hook() {
        unsafe {
            std::mem::forget(save::on_save(|| {
                Self::instance().and_then(|data| match bincode::encode_to_vec(&*data, CONFIG) {
                    Ok(bytes) => Some(bytes),
                    Err(err) => {
                        warn!("Failed to encode save data: {}", err);
                        None
                    }
                })
            }));

            std::mem::forget(save::on_load(|load_type| {
                let save::OnLoadType::SavedData(bytes) = load_type else {
                    return;
                };

                match bincode::decode_from_slice(bytes, CONFIG) {
                    Ok((data, size)) => {
                        if size == bytes.len() {
                            *INSTANCE.write().unwrap() = data;
                        } else {
                            warn!(
                                "Archipelago save data had {} extra bytes! This probably means \
                                 that you tried to load a save file created by a different version \
                                 of the Archipelago mod, or by a different mod entirely.",
                                bytes.len() - size
                            );
                        }
                    }
                    Err(err) => {
                        warn!("Failed to load save data: {}", err);
                    }
                }
            }));
        }
    }

    /// Returns a read-only reference to the singleton [SaveData], or None if
    /// the player isn't currently loaded into a game.
    pub fn instance<'a>() -> Option<RwLockReadGuard<'a, Self>> {
        // MapItemMan is only instantiated when the player is loaded into an
        // actual game, *not* on the main menu. It's a more reliable way to
        // distinguish than whether a safe file has been loaded, because no file
        // is loaded when the player starts a new game.
        //
        // Safety: We don't actually use the man, we just check whether it
        // exists.
        if unsafe { MapItemMan::instance() }.is_ok() {
            Some(INSTANCE.read().unwrap())
        } else {
            None
        }
    }

    /// Returns a read-only reference to the singleton [SaveData], or None if
    /// the player isn't currently loaded into a game.
    pub fn instance_mut<'a>() -> Option<RwLockWriteGuard<'a, Self>> {
        // See above.
        if unsafe { MapItemMan::instance() }.is_ok() {
            Some(INSTANCE.write().unwrap())
        } else {
            None
        }
    }

    /// Returns whether this save file's Archipelago seed matches [other],
    /// indicating that this save file was created for the same Archipelago
    /// room.
    pub fn seed_matches(&self, other: impl AsRef<str>) -> bool {
        match &self.seed {
            Some(seed) => seed == other.as_ref(),
            None => false,
        }
    }
}
