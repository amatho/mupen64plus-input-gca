# mupen64plus-input-gca

Input plugin for mupen64plus using the GameCube controller adapter for Wii U and Switch.

## Installing

(Currently only available for Windows, although a Linux build will most likely work.)

**Note:** The plugin will only work for 64-bit mupen64plus. I have only tested it against [m64p](https://m64p.github.io/).

Download the plugin from [Releases](../../releases/latest) and put it in your mupen64plus folder, then select it from your mupen64plus frontend.

## Building

Build requirements:

* Cargo

Then run `cargo build` from the project root. Run `cargo build --release` to compile in release mode.

**Note:** The compiled dynamic library will have underscores in it's name, but m64p (linked above) will only look for plugins with hyphens. Just rename the file and m64p will find it.

## Contributing

Feel free to open issues or pull requests.

## License

Licensed under the MIT license, see [LICENSE](LICENSE).
