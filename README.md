<div align="center">

# OWL Control

### **Help train the next generation of AI by sharing your gameplay!**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
</div>
OWL Control is a high-performance Windows app that captures the control data from games. These datasets are fundamental to training world models that power sophisticated robots and simulations.

## About

We carefully log keyboard, mouse and gamepad inputs sent directly to the game being played into a local file synced to a small video of the game. No other windows or control input is recorded. Any other window or input - including any microphone or camera - is not captured.

OWL Control is fully open-source, so anyone can verify its inner workings by reading the code or feeding this page's link to your favorite AI language model. The software is developed and enriched by a vibrant community on [our Discord server](https://discord.gg/dX4HW9Pt7Z), and anyone is allowed to [contribute to the project](./CONTRIBUTING.md)

## System Requirements

- Windows PC system capable of running games at 60 FPS.
- Keyboard, mouse, trackball, trackpad, Wired/Wireless XBOX or Wired PS5 gamepads. PS4 controllers may be used with DS4Windows.
- A reliable internet connection. Uploading may take a long time.
- Computer games! We provide [eligible games](./GAMES.md) to veteran players!

## Setup

- [Create an account at our website](https://wayfarerlabs.ai/handler/sign-up?after_auth_return_to=%2Fhandler%2Fsign-in)
- [Download OWL Control](https://github.com/Wayfarer-Labs/owl-control/releases/latest)
- Install the app
- Generate an API key on the website
- Input the API key into the app
- Review the terms of recording
- Run a game.  Hit F5 to toggle recording on/off
- Upload the recordings by hitting the Upload button.

> [!IMPORTANT]
>
> - We don't capture your microphone or anything outside the active game.
> - We screen and filter all the data we receive, and private information is removed.
> - You can review the recordings made by the app the `Open Recordings Folder` button.

## Troubleshooting

Software known to interfere with OWL Control:

- RivaTuner Statistics Server - Often installed with MSI Afterburner
- Antivirus Software - OWL Control is not malware, but you may need to add OWL Control to your antivirus whitelist if you experience problems.

## Keybindings

|      Key/Action      | Function                        |
| :------------------: | :------------------------------ |
|        **F5**        | 🔄 Toggle recording on/off      |
| **System Tray Icon** | ⚙️ Access settings and controls |

**Note**: The recording toggle hotkey can be customized in the application settings. You can also enable a separate stop hotkey if desired.

## Contributing to AI Research

By using OWL Control, you're helping to:

- Train AI agents to understand and play games
- Develop better spatial comprehension for AI systems
- Build open datasets for the scientific research community
- Advance the field< of AI and machine learning

Scrubbed and filtered data will be made publicly available for research purposes.

## For Developers

### OWL Control is open source!

If you're interested in the technical details or want to contribute:

### Building from Source

Using _powershell_:

1. Install [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

2. Clone the repo
> ```powershell
> git clone https://github.com/Wayfarer-Labs/owl-control.git
> cd owl-control
> ```

3. Build the application to create the target directory
> ```powershell
> cargo build
> ```

4. Install `cargo-obs-build`
> ```powershell
> cargo install cargo-obs-build
> ```

5. Install the dependencies
> ```powershell
> cargo obs-build --out-dir target\x86_64-pc-windows-msvc\debug
> ```

6. Run OWL Control with:

> ```powershell
> cargo run
> ```


To build a production-ready release with an installer:

- Install [NSIS](https://sourceforge.net/projects/nsis/) to default location

- Run the build script
```powershell
build-resources\scripts\build.ps1
```

|       Need Help?       | Where to Go                                                                                  |
| :--------------------: | :------------------------------------------------------------------------------------------- |
| 🐛 **Issues or Bugs?** | Report them on our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page |
|   ❓ **Questions?**    | Visit our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page          |
|     **💬Discord**      | [Discord Server](https://discord.gg/dX4HW9Pt7Z)                                              |

<div align="center">
# OWL Control is a project by [Wayfarer Labs](https://wayfarerlabs.ai)
Building open datasets for AI research
</div>