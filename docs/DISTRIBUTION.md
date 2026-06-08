# SundayStage — Distribution & auto-update (Phase 13.2)

How signed, notarized, auto-updating builds are produced for macOS + Windows.
The pipeline is wired and will run **as soon as the repository secrets below
are set** — mirrors SundayRec's and SundayEdit's approach.

## How a release works

1. Bump the version in **both** `package.json` and
   `src-tauri/tauri.conf.json` (keep them equal).
2. Tag and push:
   ```sh
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```
3. `.github/workflows/release.yml` builds on macOS + Windows, signs +
   notarizes, and creates a **draft** GitHub Release containing the installers
   and the updater manifest `latest.json`.
4. Review the draft, then **publish** it. Installed apps check the
   `releases/latest/.../latest.json` endpoint (configured in
   `tauri.conf.json` → `plugins.updater`) and offer the update via
   `UpdateBanner`.

## Updater signing key

- Keypair generated with `tauri signer generate`.
- **Private key lives OUTSIDE the repo:** `~/.tauri/sundaystage_updater.key`
  (empty password). Never commit it.
- Only the **public key** is committed, embedded in `tauri.conf.json` →
  `plugins.updater.pubkey`.
- If the private key is lost, existing installs can no longer auto-update —
  back it up somewhere safe (password manager / secure storage).

## Required GitHub repository secrets

Set these under **Settings → Secrets and variables → Actions**.

### Updater (required for auto-update to work)

| Secret                               | Value                                          |
| ------------------------------------ | ---------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Contents of `~/.tauri/sundaystage_updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key password (empty string if none)        |

### macOS code signing + notarization

| Secret                       | Value                                                      |
| ---------------------------- | ---------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | base64 of the "Developer ID Application" .p12              |
| `APPLE_CERTIFICATE_PASSWORD` | password for the .p12                                      |
| `APPLE_SIGNING_IDENTITY`     | e.g. `Developer ID Application: Richard Fossland (TEAMID)` |
| `APPLE_ID`                   | Apple ID email                                             |
| `APPLE_PASSWORD`             | app-specific password for notarization                     |
| `APPLE_TEAM_ID`              | Apple Developer Team ID                                    |

### Windows code signing

Not yet wired. Options (pick one, then add the matching secrets +
`tauri-action` inputs):

- **Standard / EV certificate** via a signing service, or
- **Azure Trusted Signing** (cheapest path to SmartScreen reputation).

Until then, Windows builds are produced unsigned (users see a SmartScreen
warning on first run).

## Deliberately deferred

- Windows code-signing certificate + wiring (above).
- Universal / Intel-mac builds (currently arm64 macOS only).
- Beta update channel (stable only for now).
- End-to-end update test: install an old build, publish a new one, confirm the
  banner downloads + relaunches on both platforms. **This is the one piece
  that can only be verified natively — do it before the first public release.**
- Branded DMG background image (layout coordinates are set; artwork pending).
