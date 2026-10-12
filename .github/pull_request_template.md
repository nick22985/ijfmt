## What changed

<!-- The formatting behaviour this changes, and why. Link the issue if there is one. -->

## Files to test against

<!--
Formatter changes are run against the full fixture suite, including private
corpora. Attach the Java files that show the problem, renamed to .txt or zipped,
or commit them as fixtures below. Include the style XML if it is not bundled.
-->

## Fixtures

- [ ] Added `Name.input.java` and `Name.expected.java` under `tests/fixtures/<style>/`, where `<style>` is the lowercased style file name in `tests/fmt/`
- [ ] `Name.expected.java` comes from IntelliJ's Reformat Code with that style, not from ijfmt or by hand
- [ ] No existing `.input.java` or `.expected.java` file was edited
- [ ] The files contain no secrets, credentials or personal data, and I have the right to share them

## Checks

- [ ] `cargo test --locked --release --test integration_test` passes
- [ ] `bash scripts/check.sh` passes
