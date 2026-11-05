@AGENTS.md

## Notes for Claude

- Keep README.md's feature table, docs/DESIGN.md and AGENTS.md current when features or conventions change.
- After pushing, check the GitHub Actions run, and tell the owner which `build-N` to install (or that the in-app updater will offer it).
- The owner isn't a developer: explain in plain words what changed, what to try on the phones, and which build. Say plainly what couldn't be verified here (Android audio/video, live sites).

## Working here (lessons so far)

- **Look at the UI before shipping it:** `cd app && SCREENSHOT_DIR=<dir> flutter test` writes PNGs from `test/home_test.dart` and `test/call_test.dart` (light and dark); open them and check. Emoji show as boxes there (no emoji font); that's fine.
- **Disk space is limited:** use `CARGO_INCREMENTAL=0`; `cargo clean` frees ~16 GB if a build fails for space.
- **Parallel helpers** work in git worktrees under `.claude/worktrees/` (ignored). Give each a disjoint set of files; after merging, resolve conflicts in generated bindings by regenerating them (`flutter_rust_bridge_codegen generate`), then rebuild the host library. Remove the worktree afterwards.
- **CI builds one release at a time, in order:** a running build is never cancelled. Pushes that arrive meanwhile wait; if several do, only the newest runs and the ones in between are cancelled. Pushes that only touch documentation (`docs/`, Markdown files) or the website don't build at all. The changelog rolls skipped, cancelled or failed builds' commits into the next one. Avoid pushing many tiny commits in a row: each one that builds is a full release.
- **Kotlin can't be compiled here** (no Android SDK). Helpers have checked Kotlin by compiling against a stand-alone Android framework jar plus Flutter's `flutter.jar`; androidx code still only gets checked in CI. Watch the CI run after Kotlin changes.
- **Product decisions to keep:** Tor is the default for everything; the faster direct mode ("Faster calls") is opt-in and says what the other side learns. Versioning: the app is **beta-1.0.0** until the owner says to move to official versions, shown with the build number as a fourth part (`beta-1.0.0.66`, the owner's choice); releases are still tagged `build-N`. The logo is the Sanctuary door, gold on midnight (`docs/brand/`). No "Mailbox" (spare always-on phone holding messages): the owner ruled it out; offline delivery comes from group members syncing with each other. Reels was built and removed in beta-1.0.0 (owner: no good algorithm, and the content was not as good as Instagram); don't bring it back unless asked.

