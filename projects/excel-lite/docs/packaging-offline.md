# macOS Packaging and Offline Verification

Excel Lite is packaged with Tauri v2 for macOS. The production bundle is configured for local `app` and `dmg` targets only; local development still uses Vite through the loopback-only `devUrl`.

Run the packaging/offline check after building frontend assets:

```sh
npm run build
npm run verify:offline
```

The verifier checks:

- `xlite-app/tauri.conf.json` keeps bundling active, targets macOS `app`/`dmg`, uses only local frontend assets, and does not enable updater behavior.
- `xlite-app/capabilities/default.json` stays at the minimal `core:default` permission set.
- `ui/dist` and any built `.app` text assets do not reference remote runtime endpoints or assets. Standards namespace URLs and Svelte diagnostic documentation strings embedded by framework runtime code are treated as metadata; loopback is allowed only for the dev-only `build.devUrl`.
- `examples/first-run-sample.xlite` is a deterministic native workbook and is listed as a Tauri bundle resource.
- If a `.app` exists under `target/**/bundle/macos`, the sample workbook is present in `Contents/Resources`.

To produce local macOS artifacts:

```sh
npm run tauri:build
```

Unsigned local builds may succeed on developer machines. Release signing and notarization still require a macOS release environment with the appropriate Apple signing identity and credentials; do not treat a local unsigned build as a notarized release.

In the local validation environment for this branch, `npm run tauri:build` produced both bundle artifacts:

- `target/release/bundle/macos/Excel Lite.app`
- `target/release/bundle/dmg/Excel Lite_0.1.0_aarch64.dmg`

`hdiutil verify` reported a valid DMG checksum. `codesign -dv --verbose=4 "target/release/bundle/macos/Excel Lite.app"` reported `Signature=adhoc` and `TeamIdentifier=not set`; `codesign --verify --deep --strict --verbose=4` and `spctl --assess --type execute --verbose=4` both failed with `code has no resources but signature indicates they must be present`. A release machine should perform real Developer ID signing and notarization after bundling.
