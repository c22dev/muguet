# Installing Muguet

Muguet doesn't come with the game. You need your own copy of the game files as a `.zip`: the folder that holds `data/` and `b/<build>/` (with `game.wasm` and `shaders/` inside).

- **Mac:** download the ready-made app, then import your game `.zip` in it.
- **iPhone / iPad:** there is no ready-made `.ipa`. You build one on a Mac from the same `.zip`, then install it with AltStore, or straight from the Mac with a cable.

## Mac

You need a Mac with Apple Silicon (M1 or later) and macOS 14 or later.

1. Download `muguet-macos-arm64.zip` from the [latest release](https://github.com/c22dev/muguet/releases/latest). It's a `.zip`, not a `.dmg`.
2. Double-click it to unzip, then drag `Muguet.app` into **Applications**.
3. Open Muguet. If macOS says it can't be opened or can't check it for malicious software, go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway**.
4. In Muguet, click **Import game data .zip** and choose your game `.zip`. This takes a while; the zip is copied into the app.
5. Click **Play**. The first launch compiles the game for your Mac, which takes a few minutes; later launches are fast.

You can also build the Mac app yourself (see below, and pick **Muguet for this Mac**).

## iPhone / iPad

You need:
- a Mac with Apple Silicon and **Xcode 26 or later** (free on the App Store);
- an **Apple ID** (a free one works);
- an iPhone or iPad on iOS 15 or later, ideally with 8 GB of RAM or more;
- about 15 GB of free space on the Mac;
- your game `.zip`, on the Mac.

### 1. Build the app on your Mac

1. Open Xcode once so it finishes installing, then sign in with your Apple ID in **Xcode → Settings → Accounts**.
2. Open **Terminal** and paste:
   ```
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/c22dev/muguet/main/build.sh)"
   ```
   It installs anything missing (Homebrew, Rust and a few tools) and may ask for your Mac password.
3. When asked what to build, pick **Muguet for iPhone / iPad**.
4. When asked for the game, drag your game `.zip` into the Terminal window and press Enter.
5. Choose how to install it:
   - **Make an .ipa (install it with AltStore):** you get `Muguet.ipa` in `~/Downloads/muguetOutput`. Continue with step 2a.
   - **Install it on my iPhone / iPad now:** plug the device in with a cable, unlock it and tap **Trust**. The script installs it for you. Continue with step 2b.

The first build takes 10–30 minutes.

### 2a. Install the .ipa with AltStore

1. Set up [AltStore](https://altstore.io) on your device by following its guide.
2. Get `Muguet.ipa` onto the device (AirDrop, or the Files app), then open it with AltStore, or use **My Apps → +** in AltStore.

Use AltStore. Sideloadly and SideStore may not work, see [this SideStore issue](https://github.com/SideStore/SideStore/issues/1616).

With a free Apple ID, apps installed this way stop opening after 7 days until AltStore refreshes them.

### 2b. If you installed it straight from the Mac

- If iOS asks for it, turn on **Settings → Privacy & Security → Developer Mode** and restart the device.
- If opening Muguet says **Untrusted Developer**, go to **Settings → General → VPN & Device Management**, tap your Apple ID and tap **Trust**.
- With a free Apple ID, the app stops opening after 7 days. Run the build again to reinstall it; your game data and saves stay.

### 3. Import the game files on the device

1. Copy your game `.zip` to the device: AirDrop it, or put it in the Files app.
2. Open Muguet and tap **Import game data .zip**, then pick the zip.
   If the picker doesn't work, move the zip to **Files → On My iPhone (or iPad) → Muguet**. It then shows up in Muguet as its own **Import** button.
3. When the import finishes, tap **Play**.

## Problems

- **"This build has no game code"** on iOS: the app was built without your game `.zip`. Build it again and give the script the `.zip` when it asks.
- **"this zip has no game data (data/manifest.json)"**: the `.zip` doesn't contain the game folder. It needs `data/manifest.json` and `b/<build>/game.wasm` inside.
- **The game closes or runs out of memory on iOS:** older or smaller devices may not have enough RAM. Devices with 8 GB or more work best.

Building by hand instead of with `build.sh` is described in the [README](README.md#building).
