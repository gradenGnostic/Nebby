<p align="center">
  <img width="256" height="256" alt="nebby" src="https://github.com/user-attachments/assets/0fe7b8ee-595e-4779-b304-04d092dde288" />
</p>

# Nebby

Nebby is a cross-platform launcher, recompiler, and mod manager for native-recompiled Nintendo 3DS Pokémon games.

It is designed to make recompiling, configuring, modding, and launching supported games as simple as possible. Nebby uses [3dsrecomp](https://github.com/fearkov/3dsrecomp) for native recompilation and provides a Zakuro-based compatibility backend for experimental titles before native replacements are available. Native Pokémon Moon does not silently fall back to Zakuro.

> [!NOTE]
> Nebby is an experimental project and is still under active development. It has only been tested on Linux. I don't use Windows, so Windows support is untested. Android remains in development.

## Features

- Recompile supported Nintendo 3DS games with **3dsrecomp**
- Launch and manage native-recompiled games
- **Zakuro compatibility backend** for experimental titles
- Per-game profiles and configuration
- Mod management
- Resolution and graphics settings
- Separate save profiles
- Linux desktop support
- Cross-platform architecture, with Android support in development
- Designed to support multiple Nintendo 3DS Pokémon games

## Planned Game Support

Nebby is being designed around the Nintendo 3DS Pokémon titles:

- Pokémon X
- Pokémon Y
- Pokémon Omega Ruby
- Pokémon Alpha Sapphire
- Pokémon Sun
- Pokémon Moon
- Pokémon Ultra Sun
- Pokémon Ultra Moon

Current native targets:

| Game | Compatible version | Status |
| --- | --- | --- |
| Pokémon Moon | EUR base v1.0 | Native runtime; gameplay and save/load working |
| Pokémon Alpha Sapphire | EUR Rev2 | Native preview; intro/title rendering tested, field and saves not yet verified |

Other listed titles use the experimental Zakuro backend. Listing a game does not guarantee that it boots or plays correctly.

## How It Works

Nebby manages the full process around a game:

```text
Your own compatible game dump
        ↓
      Nebby
        ↓
    3dsrecomp
        ↓
Native recompiled game code
        ↓
Native / compatibility runtime
        ↓
       Play
```

Pokémon Moon uses the native runtime with TriAevum/NRI/Vulkan rendering. It does not require the Zakuro runtime. Zakuro remains available for experimental titles.

The desktop bundle includes the generic recompilation tools and native SDK, not precompiled Pokémon game code. Game code is generated locally from your own compatible dump.

## Getting Started

From the source checkout:

```sh
cargo run --release -p nebby-ui
```

The bundled `workspace/` supplies the native SDK. Managed tools live in `tools/3dsrecomp/` and `runtimes/zakuro/`. `NEBBY_DATA_DIR` selects another data directory; `NEBBY_APP_ROOT` overrides the application resource root.

## Mods

Nebby's mod system supports game/version/platform compatibility declarations. Pokémon Moon work includes modern camera controls and an unfinished single-screen layout.

Further planned features include widescreen, 60 FPS patches, UI improvements, gameplay tweaks, and title-specific enhancements. These are development goals, not a promise that every listed mod is ready.

## Android

Nebby is being built with Android support in mind. ARM64 runtime work exists, but Android gameplay is not yet verified as working; current builds may black-screen.

The intended Android experience is:

```text
Install Nebby
    ↓
Import your own compatible game dump
    ↓
Select Pokémon game
    ↓
Play
```

The goal is to prepare native ARM64 builds without requiring compilation on the phone. Per-game home-screen shortcuts are also planned.

## Current Development

**Pokémon Moon is the primary working native target.**

Current Pokémon Moon work includes:

- Native-recompiled game execution
- Native Vulkan rendering through TriAevum and NRI
- Native keyboard/controller input and touch
- Native filesystem and save support
- Native GSP/PICA and DSP/audio handling
- Native memory, CTR services, kernel objects, scheduler, and bootstrap
- Native host window and audio output
- Resolution configuration and Nebby launcher integration
- Mod infrastructure

Alpha Sapphire is being brought up on the shared runtime. Other titles may be experimented with through Zakuro before they receive native implementations.

## Requirements

Nebby does **not** provide games, ROMs, keys, firmware, saves, copyrighted game assets, or translated retail game code.

You must provide your own compatible decrypted dump. Supported input formats include decrypted CXI/NCSD containers; region and revision must match the selected title profile.

The current prebuilt desktop bundle requires:

- Linux x86_64 with glibc 2.39 or newer, such as Ubuntu 24.04
- An X11/Wayland desktop
- A working Vulkan GPU driver

Older distributions require a source build. Bundled libraries do not replace glibc or GPU drivers.

Building native game code also requires a C/C++ compiler, Rust/Cargo, CMake 3.30 or newer, and Vulkan/SDL development dependencies. Bundled recompilation tools do not replace these build dependencies.

## Saves and Troubleshooting

Saves use separate profiles; existing saves are never imported automatically. Select the native profile root containing `savedata/` and `secure-values/`, not the individual `main` file.

Use **View log** for launch and build errors. Audio may stutter on slower hardware. Experimental titles may fail to boot or encounter unsupported services.

## Project Goals

Nebby is starting with Nintendo 3DS Pokémon games, with plans to support Nintendo DS and Nintendo Switch Pokémon games in the future.

The shared architecture covers recompilation tooling, rendering, input, filesystem, audio, mod loading, save management, and platform integration. Support for additional titles and platforms still needs implementation and testing; DS and Switch support is not available yet.

## Status

**Very experimental.**

Expect bugs, incomplete game support, breaking changes, and unfinished platform integrations.

## Credits

Nebby builds on and/or integrates work from projects including:

- [3dsrecomp](https://github.com/fearkov/3dsrecomp) — Nintendo 3DS static recompilation; upstream declares MIT in Cargo.toml
- [Zakuro](https://github.com/fearkov/zakuro) — compatibility runtime and behavioral reference
- [TriAevum](https://github.com/coccofresco/TriAevum) — native 3DS/PICA rendering and shared runtime architecture

See [licensing and component scope](LICENSE.md), [third-party notices](THIRD_PARTY_NOTICES.md), and `licenses/`. Preserve the applicable licenses, notices, and corresponding source when redistributing packages. The 3dsrecomp MIT declaration does not relicense other components.

## Legal

Nebby is an independent fan-made/open-source project and is not affiliated with Nintendo, The Pokémon Company, or Game Freak.

No copyrighted game files are included or distributed by this project. Users are responsible for providing their own legally obtained game data.
