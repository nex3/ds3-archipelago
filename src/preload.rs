use darksouls3::sprj::{GRANT_ITEM_VA, MapItemMan};

/// Begin loading various expensive singletons in a background thread. These
/// aren't guaranteed to be available by any particular point in time, but
/// starting immediately allows us to minimize hitching when they are loaded.
pub fn preload() {
    std::thread::spawn(|| {
        // Safety: We're not using it.
        let _ = unsafe { MapItemMan::get_instance() };
        let _ = *GRANT_ITEM_VA;
    });
}
