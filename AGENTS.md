# Browser access

Do not control Chrome or Safari, or request permission to control them. Use CLI/API
integrations where available. If a task requires browser-only steps, give the user
clear instructions and let them perform those steps themselves.

# Backend deployment

Production `forkware-api` deploys from `kanki6315/mealos` on `main`, with service
root `/backend`. Use a pull request and merge for code deployments; do not upload
local working trees to Railway unless the user explicitly requests that exception.
The legacy `mealos` assistant service is retired. Use only `forkware-api` for assistant requests. See `backend/README.md`.

# iOS distribution

Distribute builds through TestFlight only. Never install or launch the app directly on a physical device.

## FlightDeck simulator coordination

Use `flightdeck` (installed at `~/.local/bin/flightdeck`) for coordinated releases and simulator ownership. `flightdeck start forkware --worktree "$PWD" --mode archive` runs the existing project checks with automatically leased simulators; use `--mode testflight` only when an upload is authorized. Pass the intended worktree root explicitly, including when invoking from a subdirectory. Omitting `--worktree` selects FlightDeck's configured checkout, not the current directory.

For other simulator work, acquire a lease with `flightdeck lease acquire --project forkware --worktree "$PWD" --device UUID --owner DESCRIPTIVE_AGENT_OR_TASK_ID`. Keep the returned token, use that exact UUID for all install, launch, test, capture, boot and shutdown commands, heartbeat periodically with `flightdeck lease heartbeat --device UUID --token TOKEN`, and release in cleanup with `flightdeck lease release --device UUID --token TOKEN`. Do not use `booted`, choose another agent's device, erase a retained simulator, or assume a stale heartbeat means a device is free. Disable parallel simulator testing when using a single leased destination. The lease is cooperative: direct Xcode/simctl operations can bypass it.

Release scripts emit structured progress when launched by FlightDeck. Keep `.flightdeck/release.sh` with the release script in commits and worktrees. Persistent data remains after releasing a lease. Project-specific distribution and device restrictions elsewhere in this file still apply.


## FlightDeck managed tests

For fast development checks, run `bash .flightdeck/test-fast.sh` from the repository root. This runs the host-only suite directly, without FlightDeck, simulator leases, signing, or release gates. It exits nonzero on failure and prints its artifact directory. Ordinary editing and builds that do not use simulators also need no lease. These checks do not replace required iOS/UI tests.

For the full suite including simulator tests, run `flightdeck start forkware --worktree "$PWD" --mode test --wait-seconds 120` (use the absolute worktree root). This returns a job ID immediately. Poll `flightdeck detail JOB_ID` until terminal status; only `succeeded` means the tests passed. Use `flightdeck cancel JOB_ID` to stop a running or waiting job. Tests acquire, heartbeat, and release their simulators automatically after child processes stop; do not manually acquire or release leases for this command.

Busy devices cause a bounded wait with owners shown in job detail. On timeout the job fails without holding devices: continue host tests, or inspect `flightdeck devices` and retry with a free compatible `--device UUID` (repeat for Forkware phone/tablet) or a longer `--wait-seconds` up to 3600. Never steal a lease based on an old heartbeat. For an abandoned job, use `flightdeck recover JOB_ID` only after confirming its recorded processes stopped. Do not repeatedly retry an occupied device or treat a launch/timeout as passing tests.

Test jobs accept uncommitted changes and never archive, upload, or tag. Keep the worktree unchanged until its tests finish. Separate worktrees can test concurrently on different simulators, including while another worktree releases an app. The same worktree remains exclusive; releases still exclude other releases. Logs and results live in the returned job folder. Existing UI-test authorization requirements still apply. Manual leases are only needed for separate install/launch/debug operations. Existing worktrees must contain both `.flightdeck/test.sh` and `.flightdeck/test-fast.sh` from this integration.
