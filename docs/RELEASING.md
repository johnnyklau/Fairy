# Releasing a new version

How to ship a build that existing installs can auto-update to. Fairy checks
`plugins.updater.endpoints` in `src-tauri/tauri.conf.json` on every launch —
that URL always resolves to whatever GitHub Release is currently marked
"latest," so the steps below just need to produce a release shaped the way
the updater expects.

## One-time setup (already done)

A signing keypair lives at `C:\Users\<you>\.tauri-keys\fairy-updater.key`
(private, never commit it) and `fairy-updater.key.pub` (public — its
contents are already embedded in `tauri.conf.json`'s `plugins.updater.pubkey`).
If you ever lose the private key, every existing install becomes unable to
verify future updates and there's no recovery — back it up somewhere durable
(a password manager, not just this one machine).

## Every release

1. **Bump the version** in all four places, matching the existing pattern:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and
   run `npm install`/`cargo check` once so the lockfiles pick it up.

2. **Build, signed:**

   ```bash
   export TAURI_SIGNING_PRIVATE_KEY="C:\Users\<you>\.tauri-keys\fairy-updater.key"
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
   npm run tauri build
   ```

   `.env` files don't work for these — they must be real environment
   variables set in the shell that runs the build.

   This produces, under the path `src-tauri/.cargo/config.toml` points
   `target-dir` at:
   - `release/bundle/nsis/Fairy_<version>_x64-setup.exe` (+ `.sig`) — the
     installer itself, signed directly. (No separate `.nsis.zip` update
     artifact — despite what Tauri's own docs describe for some versions,
     this project's actual `tauri-plugin-updater` version signs the plain
     NSIS installer. Confirmed by inspecting a real build's output, not
     assumed from docs — if a future Tauri upgrade changes this, a fresh
     `npm run tauri build` will make the mismatch obvious immediately:
     `make-latest-json.mjs` will fail to find the `.sig` file it expects.)
   - `release/bundle/msi/Fairy_<version>_x64_en-US.msi` (+ `.sig`) — kept
     around for people who prefer MSI installs, not used by the updater.

3. **Generate the manifest:**

   ```bash
   node scripts/make-latest-json.mjs "What changed in this release."
   ```

   Writes `latest.json` to the repo root.

4. **Publish the GitHub Release**, tagged `v<version>` (the manifest script
   builds the download URL assuming this exact tag format):

   ```bash
   gh release create v<version> \
     "<target-dir>/release/bundle/nsis/Fairy_<version>_x64-setup.exe" \
     "<target-dir>/release/bundle/msi/Fairy_<version>_x64_en-US.msi" \
     latest.json \
     --title "v<version>" \
     --notes "What changed in this release."
   ```

   Mark it as the **latest** release (the default for the newest tag) — the
   updater endpoint always points at whatever GitHub currently considers
   latest.

5. **Refresh the local `installers/` copies and the installed exe**, same as
   before this feature existed — auto-update only helps _other_ installs;
   your own dev machine's copy still needs the usual manual refresh unless
   you'd rather just let it pick up the update the normal way on next launch.

## Verifying it actually works

Don't trust step 4 blindly — do a local dry run first, without publishing
anything real. Confirmed working end to end this way:

1. Run the steps above but stop before `gh release create`.
2. Serve `latest.json` and the `.exe` from a local static file server.
3. Temporarily point `tauri.conf.json`'s `plugins.updater.endpoints` at
   `http://localhost:<port>/latest.json`, build an "old" version pointing
   there, launch it, and confirm: popup appears with the right version →
   click → "Updating…" → relaunches as the new version.
   - Plain `http://` is rejected outright in release builds ("must use a
     secure protocol like https") — for this test only, also add
     `"dangerousInsecureTransportProtocol": true` next to `endpoints`.
     **Never ship this flag.**
4. Revert both the endpoint and the insecure-transport flag to the real
   values before actually publishing.

### A real gotcha this surfaced

If a machine has **both** an MSI install and an NSIS install of Fairy
registered (easy to end up with during development — this project ships
both formats), the updater's silent NSIS install can trigger a genuine
Windows Installer "Are you sure you want to uninstall this product?"
prompt, because NSIS tries to remove the conflicting MSI registration as
part of installing. It's not part of the intended update flow and it's
alarming to see mid-update — say no to it if it appears, then clean up the
stale MSI registration separately (Settings → Apps) rather than mid-install.
Going forward, prefer testing/updating against a machine with only the NSIS
install present, since NSIS is what the updater actually uses.
