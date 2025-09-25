## Dark Souls III Archipelago Client

**This repository is a work in progress.** It's intended to one day be a
replacement for [the existing DS3 Archipelago client], but for the time being
that's the only client that's actually usable.

[the existing DS3 Archipelago client]: https://github.com/nex3/Dark-Souls-III-Archipelago-client/

This repo represents a Rust port of the DS3 Archipelago client. This provides a
number of advantages:

* Rust is a more modern and more usable language than C++, as well as providing
  more guardrails for memory safety (although this is never a guarantee when
  working with mods).

* There are existing Rust libraries for modding From Software games, with
  [`fromsoftware-rs`] being of particular note. Although this library doesn't
  yet have robust DS3 support, its Elden Ring support provides a strong
  foundation for building that out.

  [`fromsoftware-rs`]: https://github.com/vswarte/fromsoftware-rs

* A ground-up rewrite provides the opportunity for much-needed refactoring in
  the original client's codebase.
