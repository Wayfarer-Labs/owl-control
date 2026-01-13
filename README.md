<div align="center">

<picture>
<img alt="An Owl emerging from an abstract globe of the Earth, all floating above a hand" src="assets/owl-logo.png">
</picture>

# OWL Control

### **Help train the next generation of AI by sharing your gameplay!**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>
OWL Control is a high-performance Windows app that captures control data from games. These datasets are fundamental to training world models that power sophisticated robots and simulations.

## About

We carefully log keyboard, mouse and gamepad inputs from the active game to a file synced with a mini video of the game. No other windows or control input is recorded. Any other window or input - including any microphone or camera - is not captured.

OWL Control is fully open-source, so anyone can verify its inner workings by reading the code or feeding this page's link to your favorite AI language model. The software is developed and enriched by a vibrant [community on Discord](https://discord.gg/ZgCWTGYf4E), and anyone is allowed to [contribute to the project](./CONTRIBUTING.md)

## System Requirements

- Windows device capable of running games at 60 FPS.
- Keyboard, mouse, trackball, trackpad, Wired/Wireless XBOX or Wired PS5 gamepads. PS4 controllers may be used with DS4Windows.
- A reliable internet connection. Uploading may take a long time.
- Computer games! We provide [eligible games](./GAMES.md) to veteran players!

## Setup

Watch the [Walkthrough Video](https://vimeo.com/1134400699) or follow the steps below:

- [Download OWL Control installer](https://github.com/Wayfarer-Labs/owl-control/releases/latest).
- Run the installer.
- Launch the app from your desktop or Start menu.
  - Check the bottom right corner of your screen for the turquoise OWL control icon. The app may already be open.
- [Create an account at our website](https://wayfarerlabs.ai/handler/sign-up?after_auth_return_to=%2Fhandler%2Fsign-in). The link is also in the app.
- [Generate an API key](https://wayfarerlabs.ai/dashboard).
- Copy your API key into the app and click `Continue`.
- Review the terms of recording. Only record if you agree with them.

> [!IMPORTANT]
>
> - We don't capture your microphone or anything outside the active game.
> - We screen and filter all the data we receive, and any private information is removed.
> - We will freely release our scrubbed and prefiltered data to the research community under permissive and open-source license.

## Usage

- We accept recordings of [these games](./GAMES.md) in PvE modes only.
- Hit `F5` key to switch recording on and off. Please only trigger this within a game you want to record.
  - A small overlay on a screen corner shows that the app is open and recording.
  - Position, keys, and goose-flavored notifications can be customized.
  - If your game runs slowly while recording, lower settings in `Video Encoder`, or lower the game detail or resolution.
- Recordings will be tracked in the app. Recordings ready to be uploaded are marked in yellow.
- Recordings may be too short or not have enough activity to submit. These recordings are marked with red and tagged invalid.
  - A message why they can't be accepted will appear. This information can be useful to share in [our Discord server](https://discord.gg/ZgCWTGYf4E).
- You can review your recordings by clicking its number. A window will open showing the folder contents.
  - Non-video files in this folder can be opened in Notepad or other text editors.
  - Location of the entire recordings folder can be changed with the `Move` button to the right of `Upload Manager`
- Upload recordings by hitting the `Upload Recordings` button.
  - If your connection is slow, try checking `Optimize for unreliable recordings`.

## Troubleshooting

Software known to interfere with OWL Control:

- MSI Afterburner - Causes problems with recording. Use [Steam Overlay with performance stats](https://help.steampowered.com/en/faqs/view/3462-CD4C-36BD5767) instead.
- RivaTuner Statistics Server - Often installed with MSI Afterburner. Sometimes causes conflicts.
- Antivirus Software - OWL Control is NOT malware. If you experience problems, you are safe to lower antivirus on OWL Control while problem solving.

If you run into other difficulties, write down what happened and take screenshots using [Windows' snipper tool](https://support.microsoft.com/en-us/windows/use-snipping-tool-to-capture-screenshots-00246869-1843-655f-f220-97299b865f6b), then [speak to us on our Discord server](https://discord.gg/ZgCWTGYf4E) or [open an issue on GitHub](https://github.com/Wayfarer-Labs/owl-control/issues).

> [!NOTE]
>
> You may get an `.invalid` recording that is marked as Too Long, is longer than 10 minutes, or larger than 150-200MB. If this happens, Please [speak to us on our Discord server](https://discord.gg/ZgCWTGYf4E) or [open an issue on GitHub](https://github.com/Wayfarer-Labs/owl-control/issues).

## Contributing to AI Research

By using OWL Control, you're helping to:

- Train AI agents to understand and play games
- Develop better spatial comprehension for AI systems
- Build open datasets for the scientific research community
- Advance the field of AI and machine learning

Scrubbed and filtered data will be made publicly available for research purposes.

## For Developers

### OWL Control is open source!

If you're interested in the technical details or want to contribute, please visit our [contributor guidelines](./CONTRIBUTING.md).

|       Need Help?       | Where to Go                                                                                  |
| :--------------------: | :------------------------------------------------------------------------------------------- |
| 🐛 **Issues or Bugs?** | Report them on our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page |
|   ❓ **Questions?**    | Visit our [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues) page          |
|     **💬Discord**      | [Discord Community](https://discord.gg/dX4HW9Pt7Z)                                           |

<div align="center">

# OWL Control is a project by [Wayfarer Labs](https://wayfarerlabs.ai)

Building open datasets for AI research<hr>

2025 Wayfarer Labs<br>
Trademarks `` copyright respective owners where indicated .</div>
