# mupen64plus-input-gca

An input plugin for Mupen64Plus using the GameCube controller adapter for Wii U and Switch.

## Installing

**NOTE:** The plugin will only work for 64-bit Mupen64Plus. I have only tested it against [m64p](https://github.com/loganmc10/m64p).

1. Follow the first section of [this Dolphin guide](https://dolphin-emu.org/docs/guides/how-use-official-gc-controller-adapter-wii-u#Installation) to set up your adapter (use Zadig if you are on Windows).

1. Download the plugin ZIP from [the latest release](../../releases/latest).

1. Extract the plugin into your Mupen64Plus folder, and select it from your Mupen64Plus frontend.

## Usage

Make sure that your adapter is plugged in and set up correctly before launching Mupen64Plus, otherwise the plugin will fail to load.

The current controller mapping is what you would expect, except for

* Y is C-button left
* X is C-button right
* L and Z are swapped (GC L is N64 Z and GC Z is N64 L)

## Building

Build requirements:

* Cargo
* Clang

For installing LLVM see [the `bindgen` User Guide](https://rust-lang.github.io/rust-bindgen/requirements.html).

To build the project:

```
$ git clone https://github.com/amatho/mupen64plus-input-gca
$ cd mupen64plus-input-gca
$ cargo build --release
```

The compiled plugin will be at `target/release/mupen64plus_input_gca.(dll|dylib|so)`.

**NOTE:** The compiled dynamic library will have underscores in it's name, but m64p (linked above) will only look for plugins with hyphens. Just rename the file and m64p will find it.

## Contributing

Feel free to open issues or pull requests.

## License

Licensed under the MIT license, see [LICENSE](LICENSE). For external code in `extern` (headers from the Mupen64Plus-Core API), see [LICENSES](extern/LICENSES).
