# FlightDeck release integration

The existing release entry point still works from a terminal. Managed runs additionally set `FLIGHTDECK_RUN_ID`, `FLIGHTDECK_EVENTS_FILE`, and an exact `TEST_DESTINATION` UUID. The helper emits newline-delimited JSON containing timestamp, stage, build, version and absolute archive path; human-readable logs are unchanged. No credentials are emitted.

When managed tests run, destination validation happens before project preflight, and parallel simulator testing is disabled so Xcode does not create unleased test clones. Forkware additionally requires distinct `TEST_IPHONE_UDID` and `TEST_IPAD_UDID`; its test runner accepts those environment defaults. `--skip-tests` does not require simulator IDs. Project signing, CloudKit gates and upload defaults remain with the existing release script.

FlightDeck is the lease authority. These scripts validate the supplied destination shape; they do not independently verify lease tokens. All agents must use the shared CLI and exact UUIDs as described in `AGENTS.md`. Check in the helper together with its release entry point so every worktree can source it.

Run the isolated contract checks without Xcode, credentials, simulator operations or uploads:

```sh
python3 -m unittest discover -s .flightdeck -p 'test_*.py' -v
```
