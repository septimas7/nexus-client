# Nexus Desktop

A native window for your own Nexus instance, on Windows and macOS.

The window shows the portal your instance already serves, at your instance's own
address. Nothing about Lists, Tasks, the Vault or any other feature lives in this
app, so when the platform ships something new it appears here the next time the
window loads, with no client update. The client updates itself only when the
shell itself changes: the window, the tray, the connect page, the updater.

What the client adds on top of a browser tab: a window that remembers its size
and position, a tray icon so Nexus keeps running when you close the window, start
at login, and a signed self updater.

Your sign in works exactly as it does in a browser. The session cookie belongs to
your instance's address and is marked `HttpOnly`, so the app cannot read it, and
no password, token or key is ever written to disk by this client.

## Install

### Windows

1. Download `Nexus Desktop_0.1.0_x64-setup.exe` from the GitHub Release.
2. Run it. The install is per user, so it needs no administrator rights.
3. Windows SmartScreen shows "Windows protected your PC" because the installer is
   not code signed. Choose **More info**, then **Run anyway**.
4. If Microsoft Edge WebView2 is missing, the installer downloads it. Windows 11
   and current Windows 10 already have it.

### macOS

1. Download the `.dmg` from the GitHub Release. One file covers both Intel and
   Apple silicon Macs.
2. Open it and drag Nexus Desktop to Applications.
3. On the first launch, right click the app and choose **Open**, then confirm.
   macOS asks this once because the build is not notarized. Double clicking
   without that first right click gives a "cannot be opened" message.

macOS 12 Monterey or later.

## First run

1. Launch Nexus Desktop. It asks for your instance address.
2. Type the address, for example `https://nexus.tail-net.ts.net` or
   `http://192.168.0.183:8080`. If you leave the scheme out, `https` is assumed.
   Plain `http` is accepted only for addresses on a private network (a LAN
   address, a tailnet address, or a name with no dots), and the page says so.
3. Press **Connect**. The client asks the address for `/healthz` and, when that
   answers, loads your portal and you sign in as usual.
4. Closing the window keeps Nexus running in the tray. The app tells you this the
   first time it happens. Quit from the tray menu.
5. To start Nexus with your computer, tick **Start at login** in the tray menu.
   It then starts hidden, in the tray.

The tray menu also has **Change instance...** if you move your instance or want
to point at a different one. There is one address at a time.

## Updates

* The client checks for a new version 15 seconds after launch and then every 6
  hours, and whenever you pick **Check for updates...** from the tray.
* When there is one, you get a dialog: **Install now** or **Later**. Nothing
  installs without your answer.
* **Install now** downloads it, checks the signature, installs it, and restarts
  the app. A progress page shows while it runs.
* Every download is verified against the public key built into your copy of the
  app. An artifact that does not verify is refused.
* The portal never needs a client update. Platform features arrive on their own.

If the tray says **Updates unavailable in this build**, that copy was built
without a public key, which is what happens before the one time setup below is
done. Install the next release by hand and the built in updater takes over.

## One time release setup

About five minutes, done once by the repository owner.

1. Generate the update signing key and keep the key file somewhere safe and
   backed up. Losing it means existing installs can never be updated again and
   have to be reinstalled by hand.

   ```
   npx @tauri-apps/cli signer generate -w ~/.tauri/nexus-desktop.key
   ```

   This writes `~/.tauri/nexus-desktop.key` and `~/.tauri/nexus-desktop.key.pub`.

2. In the app repository, add two **secrets**
   (Settings, Secrets and variables, Actions, Secrets):

   | Secret | Value |
   | --- | --- |
   | `TAURI_SIGNING_PRIVATE_KEY` | the whole contents of `nexus-desktop.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the passphrase you chose |

3. Add one **variable** (same page, Variables tab):

   | Variable | Value |
   | --- | --- |
   | `NEXUS_DESKTOP_UPDATER_PUBKEY` | the whole contents of `nexus-desktop.key.pub` |

   Nothing from this key pair belongs in the repository itself. The private key
   stays on your machine and in the secret; the public key is read from the
   variable at build time and compiled into the client.

4. Optional, and needed for the in app updater to work at all while this
   repository is private: create a **public** repository for release artifacts,
   for example `nexus-desktop-releases`, then add:

   | Setting | Value |
   | --- | --- |
   | variable `NEXUS_DESKTOP_RELEASES_REPO` | `your-account/nexus-desktop-releases` |
   | secret `NEXUS_DESKTOP_RELEASES_TOKEN` | a fine grained token with `contents: write` on that repository |

   The updater downloads without credentials, so it cannot read a release in a
   private repository. Until this exists, releases land in this repository, you
   download installers from its release page by hand, and the in app updater
   stays idle. The artifacts are the shell only: no portal code, no instance
   data.

## Releasing a new version

```
node clients/desktop/scripts/bump-version.mjs 0.1.1
git commit -am "desktop: v0.1.1"
git tag desktop-v0.1.1 -m "What changed in this release"
git push origin HEAD --follow-tags
```

The bump script sets the version in `package.json`, `package-lock.json`,
`src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` together. The tag message
becomes the release notes. Pushing the tag runs `.github/workflows/desktop-release.yml`,
which builds on GitHub's own Windows and macOS runners and publishes the
installers, the signed update artifacts and `latest.json`.

Installed copies pick the release up within six hours, or immediately from
**Check for updates...** in the tray.

## Caveats

* **The first build cannot update itself.** A client built before
  `NEXUS_DESKTOP_UPDATER_PUBKEY` exists has no key to verify with, so it never
  checks. Install one release by hand after the key exists and every release
  after that is automatic.
* **SmartScreen on Windows.** Removing the warning needs an Authenticode code
  signing certificate, which costs money per year. Optional, and it changes
  nothing about how the app works.
* **Gatekeeper on macOS.** Removing the first launch right click needs an Apple
  Developer account and notarization. Also optional.
* **One instance at a time.** Changing the address replaces the current one.
* **No offline mode.** With the instance unreachable the client says so and
  retries. It never shows an old copy of a page.
* **Nothing is reported anywhere.** No telemetry, no analytics, no crash
  uploads. The only addresses this app contacts are your instance and, for
  updates, the releases endpoint.

## Troubleshooting

**"That does not look like an address."** The text is not an address, or it asks
for plain `http` on a public host, or it carries a user name. Addresses look like
`nexus.example.ts.net`, `https://nexus.example.com:8443` or `192.168.0.183:8080`.

**"... did not answer." or "... refused the connection."** Work down this list:

1. Open the same address in a browser on the same machine. If that fails too, it
   is the instance or the network, not the client.
2. On a tailnet, check that Tailscale is connected on this machine.
3. Check the port. `https` defaults to 443 and `http` to 80, so an instance on
   8080 needs the port in the address.
4. Check the instance is serving: `GET /healthz` should return JSON.

**"... answered, but it is not a Nexus instance."** Something is listening at
that address but it is not Nexus, usually a router page, a different service on
the same port, or a captive portal.

**The window is stuck on "is not reachable".** It retries every 15 seconds by
itself and returns to the portal as soon as the instance answers. **Retry** tries
immediately.

**I closed the window and it vanished.** It is in the tray. Click the tray icon,
or use **Open Nexus** from its menu.

**Logs.** From the tray, **About Nexus Desktop** shows the folder. By default:

| System | Folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\com.septimas.nexus\logs` |
| macOS | `~/Library/Logs/com.septimas.nexus` |

Settings live next to them, in `nexus-desktop.json`:

| System | Folder |
| --- | --- |
| Windows | `%APPDATA%\com.septimas.nexus` |
| macOS | `~/Library/Application Support/com.septimas.nexus` |

Deleting that file resets the client to its first run state. It holds the
instance address, the window position, the last update check and one flag for the
tray hint, and never anything secret.

## For developers

```
clients/desktop/
  nexus-desktop-core/   pure logic: address normalization, health parsing,
                        navigation policy, update schedule. Unit tested on any
                        host, with no Tauri and no GUI toolchain.
  src-tauri/            the Tauri shell: window, tray, updater, commands.
  ui/                   the three local pages: connect, unreachable, updating.
  assets/icon.svg       the source for src-tauri/icons.
```

Both crates are excluded from the root Cargo workspace, so the platform's own
check lane never needs a GUI toolchain.

```
cd clients/desktop && npm ci                  # the Tauri CLI
npm run dev                                   # needs a C++ toolchain and a webview
npm run build                                 # produces the installers
npm run icons                                 # regenerate src-tauri/icons from the SVG

cd clients/desktop/nexus-desktop-core && cargo test
cd clients/desktop/src-tauri && cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

That last command works from Linux with no linker and no GUI libraries, which is
what the `check` job in the workflow runs. It needs an LLVM resource compiler on
`PATH` (or named by `RC`) because `tauri-build` compiles a Windows resource file
even for a check.
