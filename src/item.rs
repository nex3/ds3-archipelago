use darksouls3::cs::CSRegulationManager;
use darksouls3::param::{EQUIP_PARAM_GOODS_ST, EquipParam};
use darksouls3::sprj::{CategorizedItemID, ItemBuffer, ItemCategory, MAP_ITEM_MAN_GRANT_ITEM_VA};
use fromsoftware_shared::FromStatic;
use ilhook::x64::*;

use crate::save_data::SaveData;

/// Establishes hooks which ensure the items (which may be placeholders encoding
/// information relevant to Archipelago) are replaced by those which are correct
/// in-game.
pub unsafe fn hook_items() {
    let callback = |reg: *mut Registers| {
        // It's not clear what this number means, but the inner implementation
        // is skipped if it's below 1 so we do the same.
        if unsafe { *((*reg).r8 as *const i32) } < 1 {
            return;
        }

        let items = unsafe { &mut *((*reg).rdx as *mut ItemBuffer) };
        on_grant_items(items);
    };
    std::mem::forget(
        unsafe {
            hook_closure_jmp_back(
                *MAP_ITEM_MAN_GRANT_ITEM_VA as usize,
                callback,
                CallbackOption::None,
                HookFlags::empty(),
            )
        }
        .expect("Hooking MapItemMan::GrantItem failed"),
    );
}

/// A callback that's run when the player receives items in a way that would
/// make them pop up in a message on screen.
fn on_grant_items(items: &mut ItemBuffer) {
    for item in items.iter_mut() {
        if item.id.category() != ItemCategory::Goods || item.id.uncategorized().value() <= 3780000 {
            // This is a vanilla item.
            continue;
        }

        // Replace placeholders with their real equivalents.
        let row = &unsafe { CSRegulationManager::instance() }
            .expect("CSRegulationManager should be available in on_grant_items")
            .get_param::<EQUIP_PARAM_GOODS_ST>()[item.id.uncategorized().value().into()];
        if let Some((real_id, quantity)) = row.archipelago_item() {
            if let Some(ref mut save_data) = SaveData::instance_mut() {
                // Save data *should* always be loaded when the player gets an
                // item, but there's no need to crash if it's not.
                save_data.locations.insert(row.archipelago_location_id());
            }

            item.id = real_id;
            item.quantity = quantity;
            item.durability = -1;
        }
    }
}

pub trait CategorizedItemIDExt {
    /// Returns whether this ID represents an item added specifically for
    /// Archipelago.
    fn is_archipelago(&self) -> bool;
}

impl CategorizedItemIDExt for CategorizedItemID {
    fn is_archipelago(&self) -> bool {
        use ItemCategory::*;

        let id = self.uncategorized().value();
        match self.category() {
            Weapon => id > 23010000,
            Protector => id > 99003000,
            Accessory | Goods => id > 3780000,
        }
    }
}

pub trait EquipParamExt {
    /// Returns the Archipelago location ID encoded in this item's unused
    /// params.
    fn archipelago_location_id(&self) -> i64;

    /// If this parameter represents a synthetic wrapper around a local item,
    /// returns the real item ID and the quantity that should be given to the
    /// player.
    fn archipelago_item(&self) -> Option<(CategorizedItemID, u32)>;
}

impl<T: ?Sized + EquipParam> EquipParamExt for T {
    fn archipelago_location_id(&self) -> i64 {
        self.vagrant_item_lot_id() as i64
            + ((self.vagrant_bonus_ene_drop_item_lot_id() as i64) << 32)
    }

    fn archipelago_item(&self) -> Option<(CategorizedItemID, u32)> {
        if self.basic_price() == 0 {
            None
        } else {
            Some((
                (self.basic_price() as u32)
                    .try_into()
                    .unwrap_or_else(|err| {
                        panic!(
                            "invalid item ID {} found in synthetic item: {:?}",
                            self.basic_price(),
                            err
                        )
                    }),
                self.sell_value() as u32,
            ))
        }
    }
}
