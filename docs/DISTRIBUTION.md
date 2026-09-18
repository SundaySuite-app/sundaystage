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

`$ADMIN_KEY` below is the shared update Worker's admin key. One Worker serves every app, and on
the owner's Mac the key lives in the Keychain item `SundayRec telemetry admin key` (the name is
historical): `ADMIN_KEY="$(security find-generic-password -s 'SundayRec telemetry admin key' -w)"`.
Never paste it into a file, a commit, or a chat.

Promote the tag to the **beta** ring only:

```sh
curl -sS -X POST https://telemetry.sundaysuite.app/v1/admin/promote \
  -H "x-admin-key: $ADMIN_KEY" -H "content-type: application/json" \
  -d '{"app":"sundaystage","channel":"beta","tag":"vX.Y.Z-beta.N"}'
```

Promoting a `-beta.N` tag to `stable` is not something the Worker allows —
`/v1/admin/promote` refuses it with `channel_tag_mismatch`
(`allowedChannels: ["beta"]`). Promoting to **stable** is a separate,
deliberate step, done from a plain tag — see below.

### Stable releases (both rings since 2026-09)

Tag a plain `vX.Y.Z` — no different at the tag/build level than before. What
changed 2026-09 (`sunday-telemetry` PR #11, live in production): a plain tag
is now valid on **either** ring, and an official release is promoted to
**both** `stable` and `beta`, never `stable` alone. Skipping the second
promote leaves beta testers on an older build than the fleet — exactly the
gap this closed: Stage's own beta ring sat on `v0.8.0-beta.1` while `stable`
had already moved to `v0.8.0`, until the owner promoted `v0.8.0` to `beta` by
hand on 2026-09-18. (A `-beta.N` tag is unaffected by any of this — it still
goes to `beta` only, per above.)

Promote the same tag to both channels:

```sh
curl -sS -X POST https://telemetry.sundaysuite.app/v1/admin/promote \
  -H "x-admin-key: $ADMIN_KEY" -H "content-type: application/json" \
  -d '{"app":"sundaystage","channel":"stable","tag":"vX.Y.Z"}'

curl -sS -X POST https://telemetry.sundaysuite.app/v1/admin/promote \
  -H "x-admin-key: $ADMIN_KEY" -H "content-type: application/json" \
  -d '{"app":"sundaystage","channel":"beta","tag":"vX.Y.Z"}'
```

**Read back both rings** — `GET /v1/admin/channels` covers every app now;
Stage's rows are under `.apps[]`, not the top-level `.channels` (that key is
frozen to SundayRec for its existing callers). Confirm `stable` **and**
`beta` both report `vX.Y.Z` and neither is `paused`:

```sh
curl -sS https://telemetry.sundaysuite.app/v1/admin/channels \
  -H "x-admin-key: $ADMIN_KEY" | jq '.apps[] | select(.app=="sundaystage")'
```

**Byte-verify both feeds** against the tag's own manifest — the readback
above only proves the Worker recorded the right _tag_ on each channel, not
that both are serving the right _bytes_:

```sh
curl -sS https://updates.sundaysuite.app/v1/update/sundaystage/stable -o /tmp/stable.json
curl -sS https://updates.sundaysuite.app/v1/update/sundaystage/beta   -o /tmp/beta.json
curl -sS https://github.com/SundaySuite-app/sundaystage/releases/download/vX.Y.Z/latest.json -o /tmp/tagged.json
diff /tmp/stable.json /tmp/tagged.json && diff /tmp/beta.json /tmp/tagged.json
```

They must agree exactly — version, `pub_date`, and every platform's `url` +
signature — since `/v1/update/:app/:channel` serves the promoted manifest
verbatim, byte for byte, never a re-rendering of it (see the 200 case above).

If a bad official release reaches both rings, both need pausing and both need
the fix promoted back. The kill switch is the same call for either ring:

```sh
curl -sS -X POST https://telemetry.sundaysuite.app/v1/admin/channel \
  -H "x-admin-key: $ADMIN_KEY" -H "content-type: application/json" \
  -d '{"app":"sundaystage","channel":"stable","paused":true}'
```

Stage has no separate rollback runbook and no `promote-release.mjs`-style
script — this section is the whole procedure. There is no true rollback
either way: pausing stops new installs from being offered the bad manifest,
and the only way to move an install that already updated is a newer, good tag
promoted the same way as above.

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
