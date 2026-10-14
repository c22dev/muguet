# Muguet
A native Mach-O iOS/macOS port of GTA V, with multiplayer

## How does this work ?
this whole port relies on the WebAssembly compilation of the game. in a browser, that `game.wasm` runs inside the JS engine, behind WebGPU, Web Audio, web workers...
Muguet throws that layer away and runs the same `game.wasm` as a regular native app, with [wasmtime](https://wasmtime.dev) for the code and [wgpu](https://wgpu.rs) on Metal for the graphics

On macOS, the game code is compiled to native arm64 the first time you launch it, then cached. On iOS, we can't properly do that (i dont rlly want you to deal with the hassle of enabling JIT), so the game code is compiled when you build the app and baked into it

On top of that, Muguet adds a small UDP multiplayer (for other people who have the same port to play with you)

## Game files
Because I care about me not getting sued, you will need your own copy as a `.zip` : the folder holding `data/` and `b/<build>/` (with `game.wasm` and `shaders/`)

It's basically the same source as the web port.

Muguet itself will then asks for that zip

## Requirements
- Apple Silicon Mac, macOS 14 or later
- Rust toolchain
- Xcode 26 or later

For iOS, also :
- `rustup` with `rustup toolchain install 1.98.1` and `rustup target add aarch64-apple-ios --toolchain 1.98.1`
- `xcodegen` (`brew install xcodegen`)
- an Apple developer account
- a device on iOS 15 or later, ideally 8 GB of RAM

## Building
New to this ? [INSTALL.md](INSTALL.md) walks you through installing it on a Mac or an iPhone / iPad, step by step

The easy way, paste this in Terminal :
```
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/c22dev/muguet/main/build.sh)"
```
It downloads Muguet into `/tmp/muguet`, installs what's missing (Homebrew, Rust...), then asks whether you want the Mac or the iOS version and walks you through the rest. The `.app` / `.ipa` end up in `~/Downloads/muguetOutput`. Already have the repo ? Double-click `build.command`, or run `./build.sh`

You need Xcode 26 or later from the App Store. To install on your iPhone / iPad, sign in to Xcode with your Apple ID (Settings > Accounts), a free one works

By hand, macOS :
```
./make-app.sh
```
gives `build/Muguet.app`

iOS :
```
TEAM=<your team id> ./make-ios.sh game.zip --install
```
`--ipa` makes `build/Muguet.ipa` instead. With a free account, add `FREE=1` (+ a `BUNDLE_ID` of your own)

Please install this app with AltStore. This probably won't work well with Sideloadly (or even SideStore, as per [this issue])(https://github.com/SideStore/SideStore/issues/1616)

The app also works with a paid developer account.

## Multiplayer
One player hosts from the launcher (or runs `muguet-server` on any machine), the others join with that address. UDP port 47520

## Limitations
- iOS needs the increased memory limit entitlement
- Story missions are single player only

## Credits
- [divyanshugarg36](https://github.com/divyanshugarg36) for lots of iOS fixes and improvements
- [c0rmac](https://github.com/c0rmac) for fixing a heap corruption and a gpu memory leak, faster rendering, astc textures, native opus audio, gamepad flight controls and graphics presets on mac
- [Kagander](https://github.com/Kagander) for fixing the minimap
- [inj-src](https://github.com/inj-src) for decoding bc textures on all cores
- [Dbhardwaj99](https://github.com/Dbhardwaj99) for reporting real free disk space
- [dre4moff](https://github.com/dre4moff) for releasing controller input when the app loses focus
- [kjnefk](https://github.com/kjnefk) for importing your own saves

Contributions are welcome !
