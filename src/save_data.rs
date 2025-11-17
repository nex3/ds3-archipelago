use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use bincode;
use bincode::{Decode, Encode};
use darksouls3::util::save;
use log::*;

/// The singleton instance of the save data, or None if it hasn't been loaded
/// from the save file or set explicitly.
static INSTANCE: RwLock<Option<SaveData>> = RwLock::new(None);

/// The configuration for the binary encoding of the save data.
const CONFIG: bincode::config::Configuration = bincode::config::standard();

/// Data that's saved and loaded along with the player's game save.
#[derive(Debug, Decode, Encode)]
pub struct SaveData {
    /// The number of items that have been granted to the player in this
    /// particular save.
    pub items_granted: usize,

    /// The Archipelago seed this save file was last connected to. This is used
    /// to verify that the player doesn't accidentally corrupt a save by loading
    /// into it while connected to the wrong multiworld.
    pub seed: String,
}

impl SaveData {
    /// Register hooks for loading and unloading saves. These hooks are never
    /// unregistered.
    ///
    /// Safety: Follow all ilhook safety guidelines.
    pub unsafe fn hook() {
        unsafe {
            std::mem::forget(save::on_save(|| {
                Self::instance().as_ref().and_then(|data| {
                    match bincode::encode_to_vec(data, CONFIG) {
                        Ok(bytes) => Some(bytes),
                        Err(err) => {
                            warn!("Failed to encode save data: {}", err);
                            None
                        }
                    }
                })
            }));

            std::mem::forget(save::on_load(|load_type| {
                if let save::OnLoadType::SavedData(bytes) = load_type {
                    match bincode::decode_from_slice(bytes, CONFIG) {
                        Ok((data, size)) => {
                            if size == bytes.len() {
                                *Self::instance_mut() = Some(data);
                            } else {
                                warn!(
                                    "Archipelago save data had {} extra bytes! \
                                     This probably means that you tried to load \
                                     a save file created by a different version \
                                     of the Archipelago mod, or by a different \
                                     mod entirely.",
                                    bytes.len() - size
                                );
                                *Self::instance_mut() = None;
                            }
                        }
                        Err(err) => {
                            warn!("Failed to load save data: {}", err);
                            *Self::instance_mut() = None;
                        }
                    }
                } else {
                    *Self::instance_mut() = None;
                }
            }));
        }
    }

    /// Returns a read-only reference to the singleton [SaveData], or None if it
    /// hasn't been set yet (either from a save file or using [instance_mut].
    pub fn instance<'a>() -> RwLockReadGuard<'a, Option<Self>> {
        INSTANCE.read().unwrap()
    }

    /// Returns a read-write reference to the singleton [SaveData], or None if it
    /// hasn't been set yet (either from a save file or using [instance_mut].
    pub fn instance_mut<'a>() -> RwLockWriteGuard<'a, Option<Self>> {
        INSTANCE.write().unwrap()
    }
}
