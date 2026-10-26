#!/bin/bash
set -euo pipefail
# FLIGHTDECK_TEST_PROTOCOL=1
: "${FLIGHTDECK_RUN_ID:?Launch tests through flightdeck start --mode test}"
: "${FLIGHTDECK_ARTIFACTS_DIR:?Missing job artifact directory}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
out="$FLIGHTDECK_ARTIFACTS_DIR"
mkdir -p "$out"
FLIGHTDECK_REQUIRES_IPAD=1
# FlightDeck protocol 1. Sourced after release options have been parsed.
# Terminal releases do not require the FlightDeck CLI or an event file.
FLIGHTDECK_PROTOCOL=1
flightdeck_event() {
    [ -n "${FLIGHTDECK_EVENTS_FILE:-}" ] || return 0
    python3 - "$FLIGHTDECK_EVENTS_FILE" "$1" "${build:-}" "${version:-}" "${archive:-}" <<'FD_EVENT'
import datetime, json, os, sys
path, stage, build, version, archive = sys.argv[1:]
with open(path, 'a') as stream:
    stream.write(json.dumps(dict(time=datetime.datetime.now(datetime.timezone.utc).isoformat(), stage=stage, build=build, version=version, archive=os.path.abspath(archive) if archive else '')) + '\n')
FD_EVENT
}
flightdeck_require_udid() {
    if [[ ! "$1" =~ ^[[:xdigit:]]{8}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{4}-[[:xdigit:]]{12}$ ]]; then
        printf 'FlightDeck requires an explicit simulator UUID for %s\n' "$2" >&2
        return 2
    fi
}
if [ -n "${FLIGHTDECK_RUN_ID:-}" ] && [ "${run_tests:-1}" = 1 ]; then
    case "${TEST_DESTINATION:-}" in
        'platform=iOS Simulator,id='*) ;;
        *) echo "FlightDeck requires an explicitly leased simulator destination" >&2; exit 2 ;;
    esac
    flightdeck_require_udid "${TEST_DESTINATION#platform=iOS Simulator,id=}" TEST_DESTINATION
    if [ "${FLIGHTDECK_REQUIRES_IPAD:-0}" = 1 ]; then
        flightdeck_require_udid "${TEST_IPHONE_UDID:-}" TEST_IPHONE_UDID
        flightdeck_require_udid "${TEST_IPAD_UDID:-}" TEST_IPAD_UDID
        if [ "$TEST_IPHONE_UDID" = "$TEST_IPAD_UDID" ] || [ "$TEST_DESTINATION" != "platform=iOS Simulator,id=$TEST_IPHONE_UDID" ]; then
            echo "FlightDeck requires distinct iPhone/iPad leases and a matching phone destination" >&2
            exit 2
        fi
    fi
    export TEST_WORKERS=1
    export TEST_PARALLEL_ENABLED=NO
fi

# Preserve Xcode's existing terminal defaults unless an override is supplied.
flightdeck_test_args=()
if [ -n "${TEST_PARALLEL_ENABLED:-}" ]; then
    flightdeck_test_args=(-parallel-testing-enabled "$TEST_PARALLEL_ENABLED")
fi

log() { flightdeck_event "$*"; printf '\n==> %s\n' "$*"; }
cd ios
log "Swift core tests"
swift test --package-path . --scratch-path "$out/SwiftBuild"
log "iPhone UI tests"
xcodebuild test -project Forkware.xcodeproj -scheme 'Forkware' \
    -destination "platform=iOS Simulator,id=$TEST_IPHONE_UDID" -derivedDataPath "$out/DerivedData" \
    -resultBundlePath "$out/iPhone-UI-tests.xcresult" \
    -parallel-testing-enabled NO -maximum-concurrent-test-simulator-destinations 1 \
     CODE_SIGNING_ALLOWED=NO
log "iPad UI tests"
xcodebuild test -project Forkware.xcodeproj -scheme 'Forkware' \
    -destination "platform=iOS Simulator,id=$TEST_IPAD_UDID" -derivedDataPath "$out/DerivedData" \
    -resultBundlePath "$out/iPad-UI-tests.xcresult" \
    -parallel-testing-enabled NO -maximum-concurrent-test-simulator-destinations 1 \
     CODE_SIGNING_ALLOWED=NO
log "Tests complete"
