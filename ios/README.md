# Forkware for iPhone and iPad

Forkware is the displayed app name; the Xcode project and scheme are also named `Forkware`.

A universal native SwiftUI client for iOS/iPadOS 17+, using the existing recipe format and Apple-authenticated library sync, with Claude through the Rust backend and Apple PCC fallback. The Android app in `app/` is deprecated as of October 6, 2026 and is no longer developed or maintained. This Apple client is the active development target. The catalog exporter still reads the retained Android `RecipeCatalog.kt` seed data.

## Open and run

1. Install full **Xcode** with an iOS simulator runtime. Apple's standalone Command Line Tools cannot build this app.
2. Open `ios/Forkware.xcodeproj`.
3. Select the **Forkware** scheme and an iPhone or iPad simulator, then Run.
4. To install on your devices, select your Apple development team under **Signing & Capabilities**, then run on the connected device. Do not commit personal signing overrides.

`Config.xcconfig` points account and assistant requests at `https://forkware.arjunakankipati.com/api`. No server credential or assistant token is included in the app.

Core recipes, planning, groceries, pantry, favorites, notes, editing, and cooking work without signing in. Sign in with Apple to sync your private library across iPhone and iPad. Claude access uses a backend user UUID allowlist after Apple sign-in.

## Apple account and Rust backend (0.2)

Settings now offers native Sign in with Apple for `com.arjunakankipati.forkware`.
The target includes the Sign in with Apple entitlement. Refresh the development
provisioning profile after enabling the capability in the Apple Developer portal.
The default API base is `https://forkware.arjunakankipati.com/api`; the hostname
root is reserved for the marketing landing site. Override it in ignored
`ios/Config.local.xcconfig` using xcconfig's escaped slash syntax:

```xcconfig
FORKWARE_BACKEND_URL = https:/$()/your-host.example/api
```

The client appends `v1/auth/apple/challenge`, `v1/auth/apple`, `v1/auth/refresh`,
`v1/auth/logout`, and `v1/me`. Route `/api/*` to the Rust service, preserving the
prefix. The service also accepts unprefixed routes for direct Railway access.
The Rust API is deployed to Railway at this base URL. The hostname currently
routes to that service, with `/` reserved for the future marketing site. Never point this URL at the old assistant.

The app requests a five-minute challenge before opening Apple's sheet, supplies
its SHA-256 nonce and a checked state, then exchanges the Apple identity token.
Access and refresh tokens are saved together in a device-only Keychain item,
scoped to the backend URL. Concurrent refreshes share one request. A failed
refresh requires a new Apple sign-in because the single-use token may already
have rotated. GET account checks retry transient serverless wake failures;
credential exchanges are not automatically replayed. Launch and foreground
checks restore the session and check Apple credential state. Explicit sign-out
clears Keychain and attempts server revocation; an offline failure is reported.

Apple sign-in connects account, assistant, and library sync. This personal release starts
fresh: no Firestore import, Google account linking, or in-app migration is performed.
Deploy the library API before installing this build. Claude access remains separately
controlled by the backend allowlist. Account deletion and Apple's server-to-server
revocation lifecycle remain public-release prerequisites.

Run `swift test --package-path ios` for account transport/session and existing
core tests. On a signed device, verify first sign-in, cancellation, app relaunch,
foreground restoration, sign-out, revoking access in Apple settings, and offline
handling against the deployed Rust API. Apple credentials are never mocked in
production; the unit tests inject only their own HTTP transport and storage.

## Features

- All 15 original recipes, exported directly from `RecipeCatalog.kt`.
- Month calendar with selected-day meal details and a seven-day agenda, with one, two, or dinner-plus-lunch portions. Browse months or weeks, return to Today, and choose, change, remove, or cook a planned meal.
- Consolidated groceries grouped by category, pantry exclusions, and persistent checkmarks.
- Favorites, ratings, notes, manual recipes, and customization of bundled recipes as personal copies.
- Versioned `.mealosrecipe` import/export. Imports open in the editor for review; they never save automatically.
- Batched private library sync through the Rust backend using the Apple account.
- Local Recipe Assistant history, model selection, sources, generated drafts, and a copyable ChatGPT prompt.
- Printed-page scanning and photo OCR with Apple Vision/VisionKit; extracted text stays in the composer until Send.
- Cooking with large instructions, next-step previews, scaled ingredients, and persistent device-local timers.
- Two cooking columns at window widths of at least 720 points. Narrow iPad windows and accessibility text sizes use the compact layout. Navigation and window safe areas stay native.

## Data and synchronization

`Application Support/MealOS/forkware-v2.json` holds the new local snapshot. The former
`mealos-v1.json` and Firestore data are left untouched and are not read or uploaded.
Bundled core recipes remain available; personal recipes, plans, and preferences start
fresh. Writes are atomic and roll back in-memory state on failure.

Library edits and pending uploads are saved together. An upload waits 8 seconds after
the most recent edit, with a 60-second maximum during continuous editing. Backgrounding
attempts a flush within iOS's available execution time. Failed transient requests retry
with exponential delays from 8 seconds to 5 minutes while foregrounded. Definitive
validation/size rejections release the rejected batch while retaining current edits,
so correcting the library creates a new upload. Ambiguous failures keep the original
operation ID. Launch,
foreground, and **Sync now** also retry/fetch. There is no idle polling or guaranteed
background delivery. Pending uploads survive restarts.

Each batch replaces one private library snapshot (personal recipes and all user state)
with an expected server revision and a persistent operation ID. The server returns the
receipt and current snapshot in one response. Stale writes fail with 409; the app retains
local edits and offers an explicit choice of device or cloud library. Both alternatives
are retained in the account journal's `recovered` array for manual recovery. Conflict
resolution replaces the entire library; independent edits are not automatically merged.
Account caches and pending operations are isolated by backend URL and Apple user ID;
signed-out edits stay in their separate device library. Signing out hides the Apple
library without discarding pending edits.

Timers, cooking positions, and assistant chats remain device-local. This fresh-start
release does not import their old snapshot values. Timer notifications obey notification,
Silent mode, and Focus settings; cross-device cooking handoff is not implemented.

## Configuration and regeneration

The app icon uses the Forkware Enamel Pop FW ligature. Its editable Icon Composer
source is `MealOS/Resources/AppIcon.icon`, included in the app's resource build
phase. The matching `AppIcon` asset catalog contains an opaque flat fallback for
older iOS versions. After editing the vector artwork, regenerate that fallback:

```sh
swift ios/scripts/make_icon.swift ios/MealOS/Resources/Assets.xcassets/AppIcon.appiconset/AppIcon.png
```

To override the backend URL and optional signing team:

```sh
python3 ios/scripts/configure.py \
  --backend-url https://forkware.arjunakankipati.com/api \
  --team YOURTEAMID
```

The helper validates the URL and team, writes ignored `Config.local.xcconfig` overrides,
and regenerates the project. Personal signing settings belong in that local override.

After changing Kotlin seed recipes or adding native source/resource files:

```sh
python3 ios/scripts/export_catalog.py
python3 ios/scripts/generate_project.py
```

Both scripts use Python's standard library; XcodeGen and CocoaPods are unnecessary. The app has no Firebase or Google Sign-In dependency. Core logic itself has no external dependencies and is also available as the `MealOSCore` Swift package.

## Releasing to TestFlight

`ios/release.sh` follows broadcast-helper/Pitpass's release workflow: core tests,
iPhone/iPad UI tests, Release archive, App Store export/upload, then a local Git
tag for a clean release. It uses the existing Xcode project without regenerating
it, preserving signing edits made in Xcode. It does not push tags or create the
App Store Connect app record.

One-time setup:

1. Create an iOS app in App Store Connect for bundle ID `com.arjunakankipati.forkware`, SKU
   `mealos`, and your chosen available display name. Keep the bundle ID aligned
   with the backend Apple client ID.
2. Use the paid Apple Developer team in Xcode Signing & Capabilities. The script
   reads the project's team and version; `DEVELOPMENT_TEAM` and
   `MARKETING_VERSION` environment values can override them.
3. Reuse the Pitpass App Store Connect API credential configuration and this
   Mac's Apple Distribution certificate, provided they belong to the same team
   and have access to MealOS. No key needs to be copied into this repository.
4. In TestFlight, create an internal testing group, add yourself, and enable
   automatic distribution if desired. `MealOS/Info.plist` declares
   `ITSAppUsesNonExemptEncryption = NO` for the app's current exempt encryption
   use, so new builds include the export-compliance declaration. Reassess this
   value if encryption use changes. External testing additionally needs beta
   review and test details.

From the MealOS repository root, first export without uploading:

```sh
MEALOS_RELEASE_ENV=/Users/arjunakankipati/claude-projects/broadcast-helper/ios/.release.env \
  ios/release.sh --no-upload --allow-dirty
```

Then, when ready to upload:

```sh
MEALOS_RELEASE_ENV=/Users/arjunakankipati/claude-projects/broadcast-helper/ios/.release.env \
  ios/release.sh --allow-dirty
```

Omit `--allow-dirty` after committing the release changes to enable the local
`ios/v<version>-<build>` tag. The default build number is a UTC minute stamp;
use `--build N` for an explicit unique number or a second release in the same
minute. Use `--skip-tests` only after testing that exact code. Outputs are under
`ios/build/release/`; existing output directories are never overwritten.

Alternatively, supply `ASC_KEY_ID`, `ASC_ISSUER_ID`, and `ASC_KEY_PATH` in an
ignored `ios/.release.env`. Without API credentials the script uses the Apple
account signed into Xcode. `--no-upload` still permits signing/provisioning
updates in the developer account; it only disables binary upload. The profile
helper reuses or creates a MealOS App Store profile without deleting old ones.

The release script has not yet completed a real signed archive or TestFlight
upload. Before sharing a build, validate Apple sign-in, two-device library sync, the live assistant
endpoint, and real-device timer delivery.

## Running validation

With full Xcode selected (or use the explicit path below if `xcode-select` still points to Command Line Tools):

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
swift test --package-path ios
xcodebuild -project ios/Forkware.xcodeproj -scheme Forkware \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /tmp/MealOSDerivedData CODE_SIGNING_ALLOWED=NO build
```

On a Mac with only Command Line Tools:

```sh
python3 ios/scripts/test_core.py
```

That fallback compiles the actual shared Swift sources and executes the same test bodies through a small assertion adapter; it does not substitute for XCTest integration, an Apple app build, or simulator validation. Tests cover the full catalog, import validation, Android data compatibility, portion aggregation, timer persistence/restart/identity, local-file corruption, and assistant request limits.

Run the UI suite on an available iPhone and iPad:

```sh
python3 ios/scripts/run_ui_tests.py
```

The script discovers simulators, runs the shared `MealOSUITests` target on each, and saves logs, `.xcresult` bundles, and screenshots beneath a timestamped `/tmp/mealos-ui-*` directory. Pass `--iphone <UDID>` and `--ipad <UDID>` to choose devices. UI tests use a separate local data directory and disable cloud attachment in Debug builds, so they do not overwrite your library or upload test recipes.

Validation runs locally using the commands above. The release script also runs core and iPhone/iPad UI tests before archiving and uploading, unless `--skip-tests` is supplied.

Before treating this as device-ready, run these checks:

- iPhone portrait/landscape; iPad full-width, portrait, landscape, and narrow multitasking windows.
- Light/dark appearance, large Dynamic Type, VoiceOver labels, and keyboard reachability.
- Create/edit/cancel/import/export a recipe, reorder steps, and relaunch offline.
- Android-to-Apple sync with the same account, first connection to an empty account, offline edits/relaunch/reconnect, and sign-out while writes are pending.
- Scan and photo OCR; authorized and unauthorized assistant accounts; timeout and retry.
- Timer pause/resume/restart, app backgrounding/termination, denied notifications, Focus, Silent mode, and a real-device restart.

Historical simulator validation used Xcode 26.6 and iOS 26.5; current assistant builds require Xcode 27. See [the validation report](validation/2026-09-10/REPORT.md) for observed results, fixes, screenshots, and the remaining real-device/cloud checks.

## Assistant providers

Build with Xcode 27 or later. Claude works on the existing iOS 17+ deployment target through the Rust backend; PCC fallback is available only on supported iOS 27+ devices. `ForkwareBackendURL` configures account and assistant requests. Sign in with Apple in Settings for Claude access; The same Apple account authenticates library sync.

The target includes `com.apple.developer.private-cloud-compute`. Enable the approved PCC entitlement for the app identifier and regenerate signing profiles before device/TestFlight distribution. Simulator builds verify compilation but cannot establish that the entitlement, Apple quota, and cloud inference work on a real device.

Claude failures leave the conversation and current draft intact. Credit exhaustion, capacity limits, provider failures, and eligible transport failures offer a clearly labeled PCC button; account denial, invalid requests and explicit refusals do not. PCC sends a conservative 16 KB UTF-8 suffix of the conversation with the current draft, reserving context for instructions/reasoning/output. An oversized latest request is rejected rather than silently truncated. It never presents generated URLs as searched sources. If PCC is unavailable or out of quota, a persistent status explains why.

### PCC enabled

Default builds enable PCC: `FORKWARE_PCC_ENABLED = YES` in `Config.xcconfig` selects `MealOS-PCC.entitlements` and enables runtime fallback together. PCC approval is available for the app identifier; signing profiles must also include the entitlement. Set the flag to `NO` for a Claude-only build. Verify the resulting archive entitlements before distribution.

Distribute all device builds through TestFlight. Do not install or launch directly on a physical device.
