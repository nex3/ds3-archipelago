# Dark Souls III Archipelago Randomizer 4.0.0-alpha.2

This package contains the static randomizer and the DS3 mod for integrating _Dark Souls III_ into the [Archipelago] multiworld randomizer. You can download this from the "Assets" dropdown on [the Releases page]. If you're already reading this on the Releases page, it's just below this documentation. See [the Archipelago DS3 setup guide] for details on how to get this running, and the [game page] for more details on how randomization works with _Dark Souls III_.

[Archipelago]: https://archipelago.gg
[the Releases page]: https://github.com/nex3/ds3-archipelago/releases/
[the Archipelago DS3 setup guide]: https://archipelago.gg/tutorial/Dark%20Souls%20III/setup/en
[game page]: https://archipelago.gg/games/Dark%20Souls%20III/info/en

You can also check out [the changelog] for information about the changes in the latest randomizer release.

[the changelog]: https://github.com/nex3/ds3-archipelago/blob/main/CHANGELOG.md

## Differences from 3.x.x

This release is a ground-up rewrite of the DS3 Archipelago mod, which shares no code with the previous 3.x.x and 2.x.x versions. It's intended to be more usable, more reliable, easier to add new features, and easier to generalize to other From Software games. It has a number of major changes:

* This is built on Mod Engine 3, which is more reliable and actively maintained.

* There's now a dedicated in-game Archipelago overlay which displays the Archipelago message log and allows the player to change their connection settings in-game.

* There's better protection against issues like collecting items while disconnected from the server.

* Auto-equip is no longer supported.

## Acknowledgements

This release stands on the shoulders of many people who have worked tirelessly to help you have fun with random Dark Souls. In particular, it uses:

* The original Archipelago mod and server code by Marechal-L and many other contributors.

* The single-player "static" randomizer by thefifthmatt a.k.a. gracenotes, which is incredibly impressive in its own right.

* ModEngine3 by grayttierney and numerous others, which not only makes the "static" randomizer possible in the first place but also makes it easy to combine mods in creative ways.

* The `fromsoftware-rs` library by vswarte, axd1x8a, and dasaav, which provides important utilities for hooking and interacting with the game.

* All the incredible and thankless reverse engineering, documentation, and tooling work done by countless people at The Grand Archives, in the ?ServerName? discord, and across the internet.

* Debugging help and coding assistance from members of the Archipelago discord server, particularly Exempt-Medic and eldsmith.

* All the members of the DS3 Archipelago Discord channel who provide tireless support for the users of this package, most particularly SeesawEffect.
