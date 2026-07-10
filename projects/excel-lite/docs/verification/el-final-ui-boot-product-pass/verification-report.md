# EL-FINAL-UI-BOOT-PRODUCT-PASS verification

Verified on 2026-07-10 on macOS 26.4.1 (arm64), Node 22.23.1, npm 10.9.8, Rust/Cargo 1.96.1, and the installed Google Chrome application. The tested branch was `el-final-ui-boot-product-pass-d8f7f2fc`.

## Scope and implementation

- Replaced the removed Svelte 4 `new App({ target })` constructor with Svelte 5 `mount(App, { target })`; no `compatibility.componentApi` option or compatibility shim was introduced.
- Added an explicit missing-`#app` failure so an invalid host document reports a useful error.
- Fixed three acceptance-path repaint defects exposed after boot: grid snapshots/formats, the displayed selection range/active format, and row/column dimensions now have explicit reactive dependencies instead of hiding state reads behind helper calls.
- Replaced the malformed 32×32 Tauri PNG that prevented native startup with a valid PNG using the same green Excel Lite mark as the new inline favicon. The favicon also removes the browser preview's only 404 console message.

## Baseline reproduction

Commands:

```sh
npm ci
npm run dev -- --port 5173
npm install --prefix .worker_agent/browser-tools playwright-core@1.55.0 --no-save --no-package-lock
node .worker_agent/capture-baseline.mjs
```

At 1120×760, Chrome loaded the document title but `#app` had zero child elements and the page body was visibly blank. The page error was:

```text
Svelte error: component_api_invalid_new
Attempted to instantiate ui/src/App.svelte with `new App`, which is no longer valid in Svelte 5.
```

Evidence: [before-blank-1120x760.png](before-blank-1120x760.png).

## Browser preview acceptance

Commands:

```sh
npm run dev -- --port 5173
node .worker_agent/browser-acceptance.mjs
```

The acceptance driver used the installed Google Chrome binary. The Codex in-app browser registry was empty on this worker (`agent.browsers.list()` returned `[]`), so the in-app side-by-side surface was unavailable; this is a host-tool limitation, not an application limitation.

Interactions and observations:

1. Loaded the repaired app and waited for `Ready`. The app shell, 24 toolbar controls, formula bar, grid, and status bar were visible.
2. At 1120×760, the header was 98 px tall, the grid had 632 px of working height, all toolbar controls stayed inside the header, and header/grid/footer rectangles did not overlap.
3. Entered `12` in A1 and `8` in A2, then entered `=A1+A2` through the formula bar in A3. The browser-safe command client intentionally preserves formulas as raw text; native engine recalculation was therefore verified separately in Tauri.
4. Clicked B4 and pressed Shift+ArrowDown twice. The selection label updated to `B4:B6`.
5. Wrote `1\t2\n3\t4` to the system clipboard, pasted at D1, and observed the 2×2 range D1:E2. Copying that range returned the exact same TSV.
6. Pressed Cmd+Z and observed E2 clear with `Undo complete`; pressed Cmd+Shift+Z and observed `4` return with `Redo complete`.
7. Applied bold and currency formatting to A1 and observed bold `$12.00` without changing the raw formula-bar value `12`.
8. Increased column A from 118 px to 134 px and row 1 from 32 px to 40 px; both changes repainted immediately.
9. Activated Open workbook and Save workbook in browser preview. Both followed the browser-safe path and reported `File dialogs are unavailable in browser preview` without attempting a native-only API.
10. Resized to 720×650. The responsive header became 156 px tall, the grid retained 464 px of working height, all controls remained within the header, the formula bar moved below the toolbar, and the document stayed exactly 720×650 with no page-level overflow.

After the favicon fix, Chrome reported no page errors, warning/error console messages, failed requests, or HTTP responses at or above 400. The only remaining console output was Vite's development connection debug messages.

Evidence:

- [after-desktop-1120x760.png](after-desktop-1120x760.png)
- [after-constrained-720x650.png](after-constrained-720x650.png)

## Native Tauri acceptance

Commands:

```sh
npm run tauri:dev
screencapture -x -R196,64,1120,760 docs/verification/el-final-ui-boot-product-pass/after-tauri-boot-1120x760.png
npm run tauri:build
npm run verify:offline
hdiutil verify "target/release/bundle/dmg/Excel Lite_0.1.0_aarch64.dmg"
open -n "target/release/bundle/macos/Excel Lite.app"
```

The first native launch reached Tauri but aborted before window creation with `invalid icon: The specified dimensions (32x32) don't match the number of pixels supplied by the rgba argument (0)`. The checked-in 222-byte PNG declared 32×32 RGBA metadata but could not be decoded by either Tauri or the image inspection surface. Replacing it with a valid 32×32 RGBA PNG removed the panic.

After repair:

1. `tauri dev` opened an Excel Lite window at the configured 1120×760 size and displayed `Ready`; grid, toolbar, formula bar, and status bar were visually intact.
2. macOS System Events accessibility automation clicked the actual WebView grid and entered `12` into A1, `8` into A2, and `=A1+A2` into A3. AX static-text values then reported `A1=12`, `A2=8`, and `A3=20`. The screenshot independently shows the recalculated `20`.
3. The Open workbook toolbar button created a native `AXSheet` with description `open`, title `Open Excel Lite workbook`, and the expected `.xlite` open path. Escape dismissed it and the app reported `Open workbook cancelled`.
4. The release build produced both `Excel Lite.app` and `Excel Lite_0.1.0_aarch64.dmg`. The packaged application launched independently from `target/release/bundle/macos`, rendered the grid, and reported `Ready`.
5. The post-package offline verifier found `first-run-sample.xlite` inside the application resources and found no forbidden runtime URLs. `hdiutil verify` reported a valid DMG checksum.

Evidence:

- [after-tauri-boot-1120x760.png](after-tauri-boot-1120x760.png)
- [after-tauri-recalculation-1120x760.png](after-tauri-recalculation-1120x760.png)
- [after-tauri-native-open-dialog-1120x760.png](after-tauri-native-open-dialog-1120x760.png)
- [after-packaged-tauri-boot-1120x760.png](after-packaged-tauri-boot-1120x760.png)

Local packaging is ad-hoc signed (`Signature=adhoc`, `TeamIdentifier=not set`). Developer ID signing and notarization were not available on this host and are release-environment concerns; they do not affect the local boot or offline validation above.

## Validation gates

| Gate | Command | Result |
| --- | --- | --- |
| Conformance corpus | `python3 scripts/validate_conformance_cases.py` | PASS — 1,585 cases across 152 files |
| Python compilation | `python3 -m py_compile scripts/validate_conformance_cases.py scripts/verify_offline_bundle.py scripts/validate_ci_contracts.py scripts/check_rust_coverage.py scripts/test_check_rust_coverage.py` | PASS |
| CI contracts | `python3 scripts/validate_ci_contracts.py` | PASS — registry, dependency-decoupling, and coverage-contract checks |
| Coverage parser tests | `python3 -m unittest scripts/test_check_rust_coverage.py` | PASS — 6 tests |
| Rust formatting | `cargo fmt --check` | PASS |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` plus the repository CI allow-list | PASS |
| Rust/workspace tests | `cargo test --workspace` | PASS — 1,341 tests, 0 failures |
| Svelte/TypeScript | `npm run check` | PASS — 0 errors, 0 warnings |
| Frontend tests | `npm test` | PASS — 38 tests across 3 files |
| Frontend production build | `npm run build` | PASS — 3 asset chunks plus `index.html` |
| Offline bundle | `npm run verify:offline` after the package build | PASS — including bundled sample and app URL scan |
| Tauri package | `npm run tauri:build` | PASS — `.app` and arm64 `.dmg` created |
| DMG integrity | `hdiutil verify "target/release/bundle/dmg/Excel Lite_0.1.0_aarch64.dmg"` | PASS — checksum valid |
| Packaged smoke | `open -n "target/release/bundle/macos/Excel Lite.app"` plus AX status read | PASS — `Ready` |

The final whitespace/diff check and repository cleanliness check are recorded in the branch handoff after this tracked report is added.
