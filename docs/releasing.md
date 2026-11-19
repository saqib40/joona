# Releasing Joona

The [release workflow](../.github/workflows/release.yml) builds the Windows
installer and drafts a GitHub release. Installed copies of Joona update
themselves from the latest published release.

## One-time setup

### Update signing key

Joona only installs updates signed with the private key that matches
`plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json`.

1. Generate a key pair (skip this if you already have one):

   ```powershell
   npx tauri signer generate -w "$env:USERPROFILE\.tauri\joona-updater.key"
   ```

2. Put the contents of the `.key.pub` file in `plugins.updater.pubkey`.
3. Add these repository secrets (Settings › Secrets and variables › Actions):
   - `TAURI_SIGNING_PRIVATE_KEY`: the contents of the `.key` file.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password. Leave it out if the key has none.
4. Back up the private key and its password somewhere safe, such as a password
   manager. Never commit it.

If the key is lost, installed copies can't update any more, and everyone has
to download the new installer by hand. If it leaks, someone could sign an
update that Joona would install, so make a new pair and release the new public
key straight away.

### Code signing (recommended)

Windows SmartScreen warns people about unsigned installers, and some antivirus
tools distrust unsigned apps that use keyboard hooks. `scripts/sign.ps1` signs
every binary and the installer with the first of these that's set up, and
leaves them unsigned otherwise:

- **Azure Trusted Signing.** Set the secrets `AZURE_TRUSTED_SIGNING_ENDPOINT`
  (such as `https://eus.codesigning.azure.net`), `AZURE_TRUSTED_SIGNING_ACCOUNT`,
  `AZURE_TRUSTED_SIGNING_PROFILE`, `AZURE_TENANT_ID`, `AZURE_CLIENT_ID` and
  `AZURE_CLIENT_SECRET`. The workflow installs `trusted-signing-cli` when they're set.
- **A code-signing certificate** (`.pfx`). Set `WINDOWS_CERTIFICATE` to the
  file, base64-encoded (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`),
  and `WINDOWS_CERTIFICATE_PASSWORD` to its password.

[SignPath Foundation](https://signpath.org) signs open-source projects for
free, but it signs files after the build. The installer's update signature
would then have to be made again with `npx tauri signer sign`, which this
workflow doesn't do yet.

## Making a release

1. Pick the version, such as `0.2.0`, and set it in both places:
   - `npm version 0.2.0 -w apps/desktop --no-git-tag-version`
   - `version` under `[workspace.package]` in `Cargo.toml`, then run `cargo check`
     so `Cargo.lock` follows.
2. Add a `## 0.2.0` section at the top of `CHANGELOG.md`. It becomes the
   release notes, and its first line is what Joona shows when it offers the update.
3. Commit, then tag and push:

   ```powershell
   git tag v0.2.0
   git push origin main v0.2.0
   ```

   You can also start the workflow from the Actions tab.
4. The workflow checks that the tag, `apps/desktop/package.json` and
   `Cargo.toml` agree, and then:
   - builds the CPU and Vulkan (GPU) engines;
   - collects the third-party licences;
   - builds and signs the installer, and signs it for the updater;
   - drafts a release with `Joona_0.2.0_x64-setup.exe`, its `.sig`, and `latest.json`.
5. Try the installer from the draft on a clean PC or virtual machine, then
   publish the draft.

Installed copies read `releases/latest/download/latest.json`, which only
points at the newest published release. Drafts and pre-releases don't reach
anyone.

## How updates reach people

- Joona checks a minute after it starts and then every 12 hours, unless
  **Check for updates automatically** is off in Settings › About. The tray
  menu and Settings › About can check any time.
- Development builds never update themselves.
- When there's an update, the Hub sidebar and the tray menu say so. Nothing is
  downloaded until the person clicks **Update and restart**.
- Joona downloads the installer and checks its signature against the public
  key. It then stops the speech engine and local AI, runs the installer
  without questions, and starts again. Settings, history, models and keys
  are kept.

## Building a release locally

You need what [the README](../README.md#development) lists for development.
The Vulkan engine also needs the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home).

```powershell
.\scripts\build-engine.ps1 -Release
.\scripts\build-engine.ps1 -Variant vulkan -Release
node scripts/notices.mjs          # needs cargo-about
$env:TAURI_SIGNING_PRIVATE_KEY = "$env:USERPROFILE\.tauri\joona-updater.key"
$env:CI = "true"                  # signs with an empty password instead of asking for one
cd apps/desktop
npx tauri build --ci --config src-tauri/tauri.release.conf.json
```

To build without the GPU engine, add a second config that leaves it out:
`--config '{"bundle":{"externalBin":["binaries/joona-engine"]}}'` (in
PowerShell 5, put that JSON in a file and pass its path). The installer ends up
in `target/release/bundle/nsis/`.

## What the installer does

- Installs for the current Windows account only, without administrator
  rights, in `%LOCALAPPDATA%\Joona` (next to Joona's models and history) unless
  the person picks another folder.
- Installs the WebView2 Runtime if Windows doesn't have it.
- Joona starts at sign-in (Settings › General) and runs as a single instance.
  Starting it again opens the Hub.
- Uninstalling leaves settings, history and models in place, unless the person
  ticks **Delete the application data**. Joona then also removes its folders
  in `%APPDATA%\Joona` and `%LOCALAPPDATA%\Joona`, and the API keys it saved
  in Windows Credential Manager.
