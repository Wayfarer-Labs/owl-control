<div align="center">

# 🦉 OWL Control

### **Help train the next generation of AI by sharing your gameplay!**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

<p align="center">
  <strong>OWL Control</strong> is a desktop application that records gameplay footage and input data from video games<br/>
  to create open-source datasets for AI research. By using OWL Control,<br/>
  you're contributing to the development of AI agents and world models.
</p>

---

</div>

## 🎮 What is OWL Control?

OWL Control records your gameplay sessions (video + keyboard/mouse inputs) from games, using OBS behind the scenes to do the recording work. This data can be uploaded to create a public dataset that researchers worldwide can use to train AI models.

**[The games list](./GAMES.md)** is a list of the games we're most actively seeking data for. Other first- and third-person games are acceptable, but check in with us if possible.

**DISCLAIMER**: THERE ARE REPORTS OF OWL CONTROL CAUSING BANS IN MULTIPLAYER PVP GAMES - WE STRONGLY RECOMMEND USING ONLY IN SINGLE-PLAYER GAMES

## 🚀 Getting Started (User Installation)

~~The below text is supplemental, please watch the video tutorial here: [link](https://www.loom.com/share/f18451c8196a47a9a2dd7418f785cd37)~~

~~The video goes over common failure cases and shows some light debugging. OWL Control is currently in pre-release and there a lot of kinks to iron out. As such the video tutorial is extensive, and features some minor debugging. It should be watched in full before you use the software so you can both verify it is working and report correctly when it is not.~~

The video is currently being updated.

<table>
<tr>
<td width="50px" align="center">1️⃣</td>
<td><strong>Download</strong> the latest installer from the <a href="https://github.com/Wayfarer-Labs/owl-control/releases">Releases</a> page</td>
</tr>
<tr>
<td align="center">2️⃣</td>
<td><strong>Install</strong> the application for your operating system</td>
</tr>
<tr>
<td align="center">3️⃣</td>
<td><strong>Create an account</strong> or enter your <a href="https://wayfarerlabs.ai/dashboard">API key</a></td>
</tr>
<tr>
<td align="center">4️⃣</td>
<td><strong>Review and accept</strong> the data collection terms</td>
</tr>
<tr>
<td align="center">7️⃣</td>
<td>Recordings are currently manually initiated; when in any game, hit F4 to start recording, and F5 to stop (by default).</td>
</tr>
</table>

## 🛡️ Risks And Additional Information

- **Audio**: OWL Control does not record microphone inputs. It records game audio only, not all system audio.
- **Accidental Recording**: We have observed a bug where sometimes OWL Control responds to F4 (the default record button, which can be accidentally activated if one alt-f4s a game to close it) right after you close a game. In cases where this happens, it can be a good idea to quickly check the overlay. Black recordings that result from this will be filtered out of the uploaded dataset but might still upload.
- **Processing**: All data will undergo an automated vetting process to ensure we aren't using any empty recordings. We will never record your desktop.
- **Data Verification**: You can use the "Open Recordings Folder" button to check your own recordings before upload.

## ⚠️ Potential Incompatibilities

Some software and system configurations may interfere with OWL Control's recording functionality:

- **RivaTuner Statistics Server**: Often installed with MSI Afterburner, this software can cause compatibility issues with recordings. If you experience recording problems, try disabling or uninstalling RivaTuner Statistics Server.
- **Administrator Mode**: Some games require recorders to be run in administrator mode to be recorded properly. If you're having trouble recording certain games, try running OWL Control as an administrator.
- **Antivirus Software**: Some antivirus programs may interfere with the recording process or block OWL Control's functionality. You may need to add OWL Control to your antivirus whitelist.

## 💻 System Requirements

An NVIDIA GPU is recommended, but not a requirement.
Video upload can be bandwidth intensive, though we downsample videos to 360p and set an appropriate bitrate to lighten the load as much as possible.

## ⚠️ Restrictions

### Input Device Support

- **Keyboard + Mouse**: Supported
- **Wireless Xbox Controllers**: Supported
- **Wired PlayStation Controllers**: Supported
- **Wireless PlayStation Controllers**: _Not supported_
- **Other controllers (joysticks, etc)**: Potentially supported, but not recommended

### Recording Validation Criteria

Recordings are automatically validated before upload. The following conditions will cause recordings to be rejected:

**Video Requirements:**

The recording must be at least 30 seconds long, and its file size should be reasonable relative to duration (e.g. it should not be corrupted).

**Input Activity Requirements:**

- **Keyboard**: At least 10 WASD actions per minute OR at least 50 total keyboard events
- **Mouse**: The range of movement should be reasonable (e.g. not fully idle, and not all over the place)
- **Gamepad**: At least 20 total gamepad events AND at least 5 button presses per minute AND axis activity above 0.01 (prevents idle recordings)

**Important**: A recording will only be rejected if ALL three input types (keyboard, mouse, and gamepad) fail their respective validation criteria. This means you can use any combination of supported input devices.

## ⚙️ Uploading

Uploads are manual only. Data is stored locally and only uploaded when you manually press the Upload button in settings.

## ⌨️ Default Hotkeys

<div align="center">

|      Key/Action      | Function                        |
| :------------------: | :------------------------------ |
|        **F4**        | 🟢 Start recording manually     |
|        **F5**        | 🔴 Stop recording               |
| **System Tray Icon** | ⚙️ Access settings and controls |

Hotkeys for record/stop record can be changed in the applications settings.

</div>

## 🤝 Contributing to AI Research

<div align="center">
  <h3>By using OWL Control, you're helping to:</h3>
</div>

<table align="center">
<tr>
<td align="center">🤖</td>
<td><strong>Train AI agents</strong> to understand and play games</td>
</tr>
<tr>
<td align="center">🌍</td>
<td><strong>Develop better world models</strong> for AI systems</td>
</tr>
<tr>
<td align="center">📊</td>
<td><strong>Create open datasets</strong> for the research community</td>
</tr>
<tr>
<td align="center">🚀</td>
<td><strong>Advance the field</strong> of AI and machine learning</td>
</tr>
</table>

<div align="center">
  <em>✨ All collected data will be made publicly available for research purposes ✨</em>
</div>

## 💻 For Developers

> **OWL Control is open source!** If you're interested in the technical details or want to contribute:

### 🔨 Building from Source

First make sure you've installed Rust (Cargo). It is recommended to use PowerShell for this.
Winget can sometimes make a lot of this easier for you, but online installers should work. If you have any issues with setup, ask your local LLM!

The following steps only need to be done once:

```powershell
# Clone the repository
git clone https://github.com/Wayfarer-Labs/owl-control.git
cd owl-control

# Build the application to create the target directory
cargo build --bin owl-recorder

# Install `cargo-obs-build`, which is a helper for downloading the libobs dependencies in binary format
cargo install cargo-obs-build

# Install the dependencies into your target directory (you will have to do this again for a release build)
cargo obs-build --out-dir target\x86_64-pc-windows-msvc\debug
```

You can then run OWL Control with `cargo run`.

To build a production-ready release with an installer, use `build-resources\scripts\build.ps1` in PowerShell.
Note that this requires <https://sourceforge.net/projects/nsis/> to be installed in its default location.

Currently only Windows is supported, although we'll be adding more platforms in the near future.

### 🎨 Code Formatting

This project uses automated code formatting tools to maintain consistent code style:

```bash
# Format Rust code
cargo fmt
```

<div align="center">
  <em>📖 For detailed development instructions, see our <a href="docs/development.md">Development Guide</a></em>
</div>

## 📄 License

<div align="center">
  <em>This project is open source and available under the <a href="LICENSE">MIT License</a></em>
</div>

## 🙋 Support

<div align="center">

|       Need Help?       | Where to Go                                                                                  |
| :--------------------: | :------------------------------------------------------------------------------------------- |
| 🐛 **Issues or Bugs?** | Report them on our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page |
|   ❓ **Questions?**    | Visit our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page          |

</div>

---

<div align="center">
  <br>
  <strong>🦉 OWL Control</strong> is a project by <a href="https://wayfarerlabs.ai">Wayfarer Labs</a>
  <br>
  <em>Building open datasets for AI research</em>
  <br><br>
  <a href="https://github.com/Wayfarer-Labs/owl-control">⭐ Star us on GitHub</a>
</div>
