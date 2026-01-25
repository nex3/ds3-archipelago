## 4.0.0-beta.3

* Rename the `goal` value from "Lothric Castle Boss" to "Lothric Castle End
  Boss" for consistency with other regions that have multiple bosses.

## 4.0.0-beta.2

* Fix the structure of the bundled `apworld` file.

* Include a sample YAML template for the beta version of the apworld rather than
  one for the stable version.

## 4.0.0-beta.1

* Add an option to customize the Archipelago goal. This only works with the
  bundled apworld. See the documentation of `goal` in `Dark Souls III Options
  Template.yaml` for details.

* Add an option to only send death links when you die without picking up your
  bloodstain.

* Add an option to customize death link amnesty. See the documentation of
  `death_link_amnesty` in `Dark Souls III Options Template.yaml` for details.

* Fix a bug where rings would be placed in the incorrect locations.

## 4.0.0-alpha.8

* Fix a bug where the client would send empty `CreateHints` packets over and
  over any time a shop was open.

* Fix the font scaling for fatal error pop-ups.

* Properly make the overlay transparent when using Escape to close the in-game
  menu.

## 4.0.0-alpha.7

* When viewing an Archipelago item in a shop, the client will now post a hint to
  the server with the item's location.

* Fix a bug where the game crashed when the Archipelago connection closed.

* Make sure the cursor is visible when showing the user a fatal error.

* The overlay is now partially transparent when it's not in focus, and the input
  bar is hidden unless the player is in a menu and could access it.

* Added a settings menu which allows font size and overlay transparency to be
  adjusted.

## 4.0.0-alpha.6

* Properly identify the mod directory under Proton.

* Upgrade to a newer version of ME3 which should reduce enemy randomizer
  crashes.

* Improve a number of details of interaction with the Archipelago server.

## 4.0.0-alpha.5

* Fix a rare crash when obtaining weapons and/or armor.

* Don't produce a bogus seed conflict error when loading a save, leaving it,
  connecting, then starting a new file.

* Properly remove foreign items from the inventory after distributing them.

* Don't die because of our own death links.

* Put logs in a `log/` directory rather than the root of the mod directory.

## 4.0.0-alpha.4

* Fix a bug where guaranteed enemy drops appear as placeholders when picked up.

* Fix a bug where some items would have a synthetic copy appear in the inventory
  when purchased from shops.

* Fix a crash when purchasing an upgraded weapon from a shop.

* Properly grant the Path of the Dragon gesture when it's received in the local
  world.

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
