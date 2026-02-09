## Making contributions

Contributions are generally welcome, but it's a good idea to chat about them [in
the Archipelago Discord] before writing a bunch of code. Coming up with the
right design takes time, discussion, and collaboration.

[in the Archipelago Discord]: https://discord.com/channels/731205301247803413/1005246392329052220

## Building the client

All you need to build the client is itself is an up-to-date [Rust] installation.
Run `cargo build` and it will download all the dependencies and compile the
client DLL to `target/debug/archipelago.dll`.

[Rust]: https://rust-lang.org/

## Using your local client

To use the client, download [the latest release] from GitHub and extract it.
Edit the `me3-config.toml` file and look for

[the latest release]: https://github.com/fswap/ds3-archipelago/releases

```toml
[[natives]]
path = "archipelago.dll"
```

Replace `archipelago.dll` with the absolute path of your
`target/debug/archipelago.dll` file. Make sure to use forward slahes, because
backslashes will be interpreted as string escapes. Mine looks like this:

```toml
[[natives]]
path = "d:/Natalie/Code/ds3-archipelago/target/debug/archipelago.dll"
```

Run `launch-ds3-local.bat` as normal and it'll use your local DLL.

## Using a custom `DS3Randomizer.exe`

In many cases, if you're trying to modify the client, you won't need to change
anything about the static randomizer. You can just use the version that game
with the release you downloaded above and you'll be fine. But if you need to
use a local version of it as well, it's a little more complicated.

The exact set of dependencies the static randomizer needs and the build process
it uses is subject to change, so your best bet is to look at [the continuous
integration configuration] to get an idea of which repos you need to check out
and where to get it to build.

Build the `Debug (Archipelago)` configuration of the DS3Randomizer project. Once
that's built, run it from within the `randomizer` directory and it'll set up an
`apconfig.json` in the appropriate location. If you're running from within
Visual Studio, you can edit the Debug Properties of the executable and set the
Working Directory to the `randomizer` directory and then run the executable
directly from the IDE.

[the continuous integration configuration]: .github/workflows/release.yaml
