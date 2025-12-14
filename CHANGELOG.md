## 4.0.0-alpha.4

* Fix a bug where guaranteed enemy drops appear as placeholders when picked up.

* Fix a bug where some items would have a synthetic copy appear in the inventory
  when purchased from shops.

* Fix a crash when purchasing an upgraded weapon from a shop.

* Clarify the error message for a config version mismatch.

### Linux/Proton

* Properly locate the `apconfig.json` file on Linux.

* Fix the path to the ME3 config file in the `launch-ds3.sh` script for Linux.

## 4.0.0-alpha.3

* Fix a bug that was causing "object reference not set to an instance of an
  object" errors.

* Improve the static randomizer feedback for invalid enemy presets.

## 4.0.0-alpha.2

* Wait 30s after each death link before sending or receiving the next one.

* Only prompt for the URL when a connection error occurs. All other changes
  should be handled with the static randomizer.

* Display more errors as in-game overlays.

* Properly convert randomized shop items into real, usable items.

* Delay the check for the DLC until we're confident it will be accurate.

* Provide a pre-assembled Linux release.

* Create a separate DS3 save file for each Archipelago seed.

## 4.0.0-alpha.1

* Initial alpha release of the Rust client.
