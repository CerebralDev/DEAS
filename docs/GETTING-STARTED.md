# Installation, launch and first setup

[Documentation index](README.md) · [Plan](PLAN.md)

## Getting started — Windows and Linux

**DEAS 0.2.17 is BETA software.** Linux CPU/NVIDIA installation has been tested; Windows builds exist, but running the app and its installer on Windows has not yet been verified. AMD/Intel GPU installation remains blocked. Video recipes exist, with a recorded successful H3 job; Linux video playback uses the native libmpv player, while audible A/V sync, normal-driver interaction and Windows playback acceptance remain open; see [video limitations](STUDIO.md#video-generation). Use a separate folder, not an existing ComfyUI installation.

**Start here:** [Windows](#windows-step-by-step) · [Linux](#linux-step-by-step) · [First setup inside DEAS](#first-setup-inside-deas) · [Help](#if-something-goes-wrong)

### Before you start

- Use a **64-bit Intel/AMD Windows or Linux computer**. These instructions do not cover ARM devices.
- Have an internet connection and a writable folder with room for the app, its downloaded backend and your models. AI models can take several gigabytes each; there is no single storage requirement for every model.
- For NVIDIA acceleration, install a working NVIDIA driver first. DEAS checks compatibility; it does not install your graphics driver. CPU mode is available but much slower.
- You do **not** need to install Python, PyTorch, CUDA Toolkit or ComfyUI manually for a new DEAS-managed environment. DEAS downloads its managed Python and backend after you approve setup. Git is still required.

### Download or build?

Check the official [Releases page](https://github.com/darksidewalker/DEAS/releases) first. Use available release artifacts only when their platform requirements match your system. The instructions below build the app from source; unfortunately, that still requires some one-time developer tools. **GitHub's “Code → Download ZIP” downloads source code, not a ready-to-run app.**

When release downloads are available, the release workflow produces `deas-linux-amd64` for Linux, and `deas.exe` plus a Windows setup `.exe`. Follow that release's instructions; you do not need Node.js, Rust or C++ build tools just to run a prebuilt app. Linux still needs compatible system/webview libraries, and Windows needs WebView2. Download only from this project's official GitHub pages.

### Windows step by step

#### 1. Install the one-time tools

Open each official link, download the installer and run it:

| Tool | What to choose |
| --- | --- |
| [Git for Windows](https://git-scm.com/downloads/win) | Keep the option that makes Git available from the command line. |
| [Node.js](https://nodejs.org/en/download) | Choose **Node.js 22 LTS**; include npm. Version 22 is used by this project's CI. |
| [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) | Select **Desktop development with C++**, including the recommended Windows SDK and MSVC tools. You do not need the full Visual Studio editor. |
| [Rust](https://www.rust-lang.org/tools/install) | Run `rustup-init.exe` and accept the default **MSVC** installation. |
| [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download-section) | Install the **Evergreen Bootstrapper** if WebView2 is not already installed. |

Restart your computer if an installer requests it. Close any old terminal windows before continuing so the new tools can be found.

#### 2. Get DEAS and build it

1. Create a folder named `DEAS-work` in your personal user folder, somewhere you can write without administrator permission.
2. Open that folder in File Explorer. Click the address bar, type `cmd` and press Enter. This opens **Command Prompt** in that folder; use it rather than PowerShell for the commands below.
3. Copy and run these lines **one at a time**. Wait for each to finish before entering the next:

```bat
git clone https://github.com/darksidewalker/DEAS.git
cd DEAS
build-windows.cmd
```

The first build downloads dependencies and can take a while. It has finished successfully when it prints `built: artifacts\deas-windows-x86_64.exe`. If it ends with an error instead, see [Help](#if-something-goes-wrong).

#### 3. Open the app

In File Explorer, open `DEAS-work → DEAS → artifacts` and double-click **`deas-windows-x86_64.exe`**. Keep it in that writable folder for initial setup. If Windows blocks the file, check its source and the warning; do not disable antivirus or Windows security globally. This beta is not a signed production release.

Continue with [First setup inside DEAS](#first-setup-inside-deas).

### Linux step by step

The commands below use **Ubuntu 24.04** as the build baseline, matching CI. Other distributions need their equivalent packages; see [Tauri's Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux). The resulting executable is system-linked, not a universal portable Linux app.

#### 1. Install the one-time tools

Open **Terminal** from your applications menu. On Ubuntu 24.04, run each line separately:

```sh
sudo apt update
sudo apt install git build-essential pkg-config curl wget file libxdo-dev libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

`sudo` asks for your computer's login password; no characters appear while you type it. Review the package list before confirming installation.

Install **Node.js 22 LTS with npm** using the [official Node.js instructions](https://nodejs.org/en/download). Do not assume your distribution's default Node.js package is recent enough. Install Rust using the command from the [official Rust installer](https://www.rust-lang.org/tools/install):

```sh
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

This downloads and runs Rust's installer; use the official site above if you want to review the installation options first. Accept the default installation, then **close and reopen Terminal**.

#### 2. Get DEAS and build it

Run these lines one at a time in your normal user account, **not with sudo**:

```sh
cd ~
git clone https://github.com/darksidewalker/DEAS.git
cd DEAS
bash build-linux.sh
```

The first build downloads dependencies and can take a while. It has finished successfully when it prints `built: artifacts/deas-linux-amd64` followed by a checksum.

#### 3. Open the app

From the same Terminal window, run:

```sh
./artifacts/deas-linux-amd64
```

For later launches, open the `DEAS/artifacts` folder in your file manager and open `deas-linux-amd64`, or run the same command from the DEAS folder. No development web server is needed. Keep the executable in a writable location for initial setup.

### First setup inside DEAS

These steps are the same on both systems. Button names below are English; use the language selector to switch to English if you want the labels to match.

1. Open the **Backend** tab if the setup screen is not already visible. DEAS checks your hardware and prepares a recommendation; this alone installs nothing.
2. Under **Review & edit installation**, check **Storage directory**. The default is a new `deas-environment` folder beside the executable. To use another drive, choose an existing parent folder and a **new** environment subfolder. Do not point it at an existing ComfyUI installation or a folder containing your files.
3. Leave the recommended backend selected. **Director** is checked by default and includes Forge, RefMods, Continuity and Output; uncheck it if you only want core ComfyUI. Leave **Advanced options** unchanged for your first install. Unsupported options block installation rather than being skipped.
   To reuse existing models before installation, expand **Optional folders** and choose **Models folder** (Input and Output are optional too). These choices are included in the reviewed installation plan; selecting folders does not move files or start installation. The order in which you choose them does not matter. Native folder safety checks still apply.
4. Read any warnings, then click the green **Start install** button. This authorizes the displayed downloads and filesystem changes. Keep DEAS open while it downloads and checks the environment. **Logs** shows technical progress and errors.
5. On success, DEAS opens **Studio**. Its managed ComfyUI engine (the “backend”) starts automatically by default. Wait for **Backend → Started**; if automatic startup is disabled, open **Backend** and click **Start**. You can change **Auto-start backend** there.
6. **Install model files separately.** Setup does not include model weights. Use **Library** to browse and download files, or use existing model folders configured under **Backend**. Follow the chosen model's license and folder requirements; some models need additional text encoders and a VAE, not just one checkpoint. Provider authorization is optional for public files, but gated files may need a token and license acceptance on the provider's website.
7. In **Studio → Generate**, choose a supported image model/family and provide its required files, enter a prompt, then use **Run** when prerequisites are satisfied. Alternatively, **Comfy** opens the original ComfyUI workflow editor for the same backend. Only compatible operations are enabled; video quality/runtime coverage and playback remain limited. A successful installation does **not** guarantee every model or workflow will work.

**Next time:** open the same DEAS executable. The last checked DEAS environment is restored without reinstalling, and its backend follows your auto-start setting. Keep the environment folder; it contains the managed installation and may contain your models and media. **Use existing environment** is for reopening a completed DEAS installation. Adopting a foreign ComfyUI folder is a separate, destructive option (it rebuilds that folder's venv); see [Backend](BACKEND.md#adopting-a-foreign-installation).

### If something goes wrong

| What you see | What to do |
| --- | --- |
| `git`, `npm` or `cargo` not found / not recognized | Install the matching tool above, then close and reopen the terminal. Check with `git --version`, `node --version`, `npm --version` and `cargo --version`. |
| Windows build mentions a missing C++ compiler, linker or Windows SDK | Reopen Microsoft C++ Build Tools and install **Desktop development with C++**, including its recommended components. |
| Linux build mentions `webkit2gtk-4.1`, OpenSSL or `pkg-config` | Check the Linux prerequisites above and the linked Tauri instructions for your distribution. |
| Linux download will not execute | For a downloaded release binary, open Terminal in its folder, run `chmod +x deas-linux-amd64`, then `./deas-linux-amd64`. A missing shared-library error needs compatible system packages, not another permission change. |
| “This folder holds an unfinished DEAS installation” | An earlier install of that folder failed or was cancelled. Choose a new environment subfolder; Setup then prepares a fresh plan. Press **Start install** again. DEAS never deletes or resumes the old folder; remove it yourself once you no longer need its logs. |
| “uv download stalled” | No data arrived for 60 s (or the download passed 15 minutes). Pause other large downloads (game launchers, cloud sync) or check the connection, then retry with a new environment subfolder. |
| Storage folder already exists / permission denied | Choose a writable existing parent and a new environment subfolder. Do not delete an existing installation or run DEAS as administrator/root to bypass the check. |
| Windows Verify fails with `signal` / `alarm` AttributeError | This is an installer bug, not evidence of a CUDA failure. Use a build containing the native probe-timeout fix. Failed partial environments are retained; automatic repair/resume is not implemented. Keep the old folder and choose a new environment subfolder for a fresh retry. |
| Setup reports an unsupported backend or option | Leave optional acceleration packages off. Use the supported NVIDIA recommendation or CPU; AMD/Intel GPU installation is currently blocked. |
| Studio has no models / Run is unavailable | Backend installation and model downloads are separate. Check **Backend**, model files and the selected recipe's reported prerequisites. |
| Comfy says “Waiting for backend…” | Open **Backend**, check its status and click **Start** if stopped. Read **Logs** if startup fails. |

If you are stuck, [open a GitHub issue](https://github.com/darksidewalker/DEAS/issues) with your operating system, GPU, DEAS version and the relevant error from the build terminal or **Logs**. Remove provider tokens and other private information before posting. See [verification and known limitations](VERIFICATION.md) for what has actually been tested.
