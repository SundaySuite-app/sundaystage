# SundayStage — Distribution & auto-update (Phase 13.2)

How signed, notarized, auto-updating builds are produced for macOS + Windows.
The pipeline is wired and will run **as soon as the repository secrets below
are set** — mirrors SundayRec's and SundayEdit's approach.

## How a release works

1. Bump the version in **both** `package.json` and
   `src-tauri/tauri.conf.json` (keep them equal).
2. Write `docs/release-notes/vX.Y.Z.md` — the text the operator reads in the
   in-app update banner. CI refuses the PR without it, and `release.yml`
   refuses the tag. See [release-notes/README.md](release-notes/README.md) for
   why it lives in the repo and what the rules are.
3. Tag and push:
   ```sh
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```
4. `.github/workflows/release.yml` builds on macOS + Windows, signs +
   notarizes, and creates a **draft** GitHub Release containing the installers
   and the updater manifest `latest.json`.
5. Review the draft, then **publish** it.
6. **Promote** the build to a ring on the shared update Worker (below).
   Publishing alone reaches nobody from v0.5.0 onward.

## Update rings (since v0.5.0 / E2)

Installed apps poll the app-scoped rings on the shared Sunday update Worker:

| Ring   | Endpoint                                                       |
| ------ | -------------------------------------------------------------- |
| stable | `https://updates.sundaysuite.app/v1/update/sundaystage/stable` |
| beta   | `https://updates.sundaysuite.app/v1/update/sundaystage/beta`   |

- A ring answers **200** with the ordinary Tauri manifest (byte-identical in
  shape to `latest.json`, signed with the **same** key — the pubkey in
  `tauri.conf.json` did not change), **204** when nothing is promoted or the
  ring is paused, **404** for an unknown ring. 204 means "up to date", never an
  error — it is also the kill switch.
- Which ring an install follows is a per-machine setting: **Settings →
  Advanced → Update channel**. Default stable; beta is a two-way door. It
  applies from the next check (the endpoint is resolved per check).
- The check itself runs in Rust (`commands::updater`), because
  `UpdaterBuilder::endpoints(..)` is the only seam that can choose an endpoint
  at runtime — the JS `check()` cannot. `tauri.conf.json` keeps the stable ring
  as the configured fallback, pinned equal by a unit test.
- **The 0.4.0 fleet is still on GitHub.** Those installs poll
  `releases/latest/download/latest.json`, so the workflow keeps uploading it
  (`uploadUpdaterJson: true`) and beta tags are marked as GitHub prereleases so
  they can never become "Latest" for that fleet. **v0.5.0 is the 0.4.0 fleet's
  last GitHub hop**; from 0.5.0 onward everything goes through the rings.

### Beta releases

Tag `vX.Y.Z-beta.N`. The workflow then:

- marks the GitHub release as a **prerelease** (never "Latest"), and
- builds **NSIS only** on Windows — an MSI `ProductVersion` is a numeric triple
  with nowhere to put `-beta.1`, and the bundler hard-fails on it.

Promote the tag to the **beta** ring only; promote to **stable** as a separate,
deliberate step.

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
- End-to-end update test: install an old build, publish a new one, confirm the
  banner downloads + relaunches on both platforms. **This is the one piece
  that can only be verified natively — do it before the first public release.**
- Branded DMG background image (layout coordinates are set; artwork pending).
