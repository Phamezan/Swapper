# Releasing Swapper

Swapper ships as a signed-for-updater Windows NSIS installer built by the
[Windows Installer workflow](../.github/workflows/windows-installer.yml). The
installed app checks
`https://github.com/Phamezan/Swapper/releases/latest/download/latest.json`
after startup and about every six hours, and offers **Update & Restart**.

## 1. One-time setup: generate the updater signing key

The updater refuses builds it cannot verify, so every release artifact is
signed with a minisign key pair generated once with the Tauri CLI:

```sh
npx @tauri-apps/cli signer generate -w ~/.tauri/swapper.key
```

- The command prints the **public key** and writes the **private key** to the
  file given with `-w` (it is protected with the password you choose).
- Never commit the private key. Keep a backup — losing it means users must
  reinstall manually, because updates signed with a new key are rejected.

Put the printed public key into `src-tauri/tauri.conf.json`, replacing the
placeholder:

```json
"plugins": {
  "updater": {
    "pubkey": "<the public key>",
    ...
  }
}
```

While `pubkey` still reads `REPLACE_WITH_UPDATER_PUBLIC_KEY`, update checking
stays off in the app: no checks run, no errors are shown, and Swapper Doctor
reports "Update checking is not configured in this build."

## 2. Repository secrets

Add two GitHub secrets (*Settings → Secrets and variables → Actions*):

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of the private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key's password (omit the secret if the key has none) |

## 3. Cut a release

1. Bump the version in all three places: `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml`, and `package.json`. Commit and push.
2. Create an **annotated** tag whose message is the release notes, then push it:

   ```sh
   git tag -a v0.5.0 -F notes.md   # or -m "short notes"
   git push origin v0.5.0
   ```

3. The **Windows Installer** workflow runs on the tag and publishes the GitHub
   release `Swapper vX.Y.Z` with the installer, `latest.json`,
   `Deceive-v1.18.0-source.zip` and `SHA256SUMS.txt`. The tag message becomes
   both the release body and the notes shown in Swapper's update prompt.

The workflow refuses to publish when the tag does not match the version in
`tauri.conf.json`, when the tag is lightweight (no notes), or when
`TAURI_SIGNING_PRIVATE_KEY` is missing — a release without `latest.json` would
become "latest" and break update checks for every installed copy.

Running the workflow manually (workflow_dispatch) still builds without
publishing, for testing an installer.

Installs older than the first updater-enabled release have no updater: users
install that release by hand once, and update in-app from then on.

## 4. Where user data lives

Swapper saves accounts, vault, profile-icon cache and Remote Control state
under `%LOCALAPPDATA%\Swapper` — the same folder the per-user NSIS installer
installs to. This is safe for updates and uninstalls; the evidence below is
from the NSIS template the release toolchain actually uses
(`tauri-bundler` as bundled in `@tauri-apps/cli` 2.11.5, tag `tauri-cli-v2.11.5`,
`crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi`):

- **Install directory.** For the default `currentUser` install mode the
  installer picks the user-local folder (lines 499–515):

  ```nsis
  !else if "${INSTALLMODE}" == "currentUser"
    StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
  ```

  so the install directory is `%LOCALAPPDATA%\Swapper`, the same folder the
  app writes its data to.

- **Updates never uninstall.** The updater launches the installer with
  `/UPDATE` (lines 488–491 set `$UpdateMode`), and the reinstall page then
  skips the uninstall step outright (lines 318–320):

  ```nsis
  ; In update mode, always proceeds without uninstalling
  ${If} $UpdateMode = 1
    Goto reinst_done
  ${EndIf}
  ```

  The install section only writes the installer's own files — the main
  executable, bundled resources and external binaries (`SetOutPath $INSTDIR`,
  `File "${MAINBINARYSRCPATH}"`, lines 639–650) — plus a cleanup of a renamed
  main binary (`Delete "$INSTDIR\$OldMainBinaryName"`, line 694). It never
  enumerates or clears unknown files, so `state.json`, `vault/`, `icons/`,
  `lan-paired-devices.bin` and `remote-route.json` are not touched.

- **The uninstaller deletes only its own file list**, then tries a
  non-recursive directory removal (lines 780–812): `Delete
  "$INSTDIR\${MAINBINARYNAME}.exe"`, the `{{#each resources}}` and
  `{{#each binaries}}` lists, `Delete "$INSTDIR\uninstall.exe"`, and finally

  ```nsis
  RMDir "$INSTDIR"
  ```

  There is no `RMDir /r "$INSTDIR"` anywhere in the uninstall section, and
  `RMDir` without `/r` only removes an empty directory. A default uninstall
  therefore leaves the folder — and with it every saved account — on disk.

- **The "delete app data" checkbox removes only the identifier folders.** The
  checkbox state is read on leaving the confirm page (lines 459–461), and the
  uninstall section honours it only when not updating (lines 870–883):

  ```nsis
  RmDir /r "$APPDATA\${BUNDLEID}"
  RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ```

  `${BUNDLEID}` is the bundle identifier (`!define BUNDLEID "{{bundle_id}}"`,
  line 54) — for Swapper `com.phamezan.swapper`, where WebView2 keeps its
  profile. The data folder `%LOCALAPPDATA%\Swapper` is not in that list, so
  even a full uninstall with the checkbox ticked leaves saved accounts intact.

Net effect: updates and uninstalls (default or with "delete app data") never
remove Swapper's saved data. The trade-off is that an uninstall leaves a
leftover `%LOCALAPPDATA%\Swapper` folder containing the data; a later
reinstall picks the accounts back up. Keep this in mind when changing the
bundle identifier: the delete-app-data folders are keyed to it.
