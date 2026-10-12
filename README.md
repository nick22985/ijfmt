# ijfmt

A fast Java formatter, written in Rust, that reads IntelliJ IDEA code style XML
files and aims to produce byte-for-byte the same output as IntelliJ's own
formatter.

- Uses your existing IntelliJ code styles, with no translation step.
- Formats a file, a list of files, a whole project, or stdin.
- Can reformat only the lines you changed since a git revision.
- Formats files in parallel and writes them atomically.

## Install

Requires a Rust toolchain that supports edition 2024.

```sh
./install                          # build, install binary and bundled styles
./install --default-style Sample   # also make Sample the global default style
./install --dry-run                # show what would happen
```

The binary goes to `~/.local/bin` and styles to `~/.config/ijfmt/styles/`,
installed under their IntelliJ scheme name. Run `./install --help` for
`--bin-dir`, `--styles-dir`, `--no-bin` and `--no-styles`.

To build without installing:

```sh
cargo build --locked --release     # target/release/ijfmt
```

## Usage

```sh
ijfmt < MyClass.java > Formatted.java   # stdin to stdout
ijfmt --file MyClass.java               # one file, in place
ijfmt First.java Second.java            # several files, in place
ijfmt --project                         # every *.java under the current directory
ijfmt --check --project                 # report, write nothing
```

Add `--style path/to/Style.xml` to any command to pick a style explicitly.
Otherwise ijfmt discovers one (see [Style discovery](#style-discovery)).

| Option | Effect |
| --- | --- |
| `-s, --style FILE` | Use this style XML |
| `--fallback FILE` | Style to use when discovery finds none |
| `--file FILE` | Format one file in place; style discovery starts from its directory |
| `--project` | Format every `*.java` below the current directory, skipping hidden directories and symlinks |
| `-c, --check` | Write nothing; exit 1 if anything would change |
| `--changed[=REF]` | Only reformat lines changed since `REF` (default `HEAD`) |
| `--malformed-java preserve\|repair` | Keep malformed constructs (default), or drop recoverable duplicates first |

`--file`, `--project` and file arguments are mutually exclusive. With none of
them, ijfmt reads stdin and writes stdout.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | `--check` found files that would change |
| 1 | Startup error, such as an unreadable style |
| 2 | A file could not be processed, or the arguments were invalid |

Batch runs keep going after individual file or directory errors and report the
worst outcome.

### Formatting only changed lines

```sh
ijfmt --changed --file MyClass.java     # changes since HEAD, staged and unstaged
ijfmt --changed=main --project          # changes since another revision
ijfmt --changed --check --project       # CI-style check of changed lines only
```

ijfmt asks git which lines differ from the revision, formats the whole file,
then keeps only the output that replaces or borders a changed line. Spacing
changes are applied per line. A statement the formatter joins or splits is taken
as a whole when any line of it changed.

- Untracked files are formatted in full.
- With `--project`, only changed and untracked `*.java` files are visited.
- Not available for stdin, since there is no file for git to compare.
- `REF` must be attached with `=`, so `--changed main` treats `main` as a file.

### Style discovery

Without `--style`, ijfmt resolves a style name in this order:

1. `PREFERRED_PROJECT_CODE_STYLE` in `.idea/codeStyles/codeStyleConfig.xml`,
   searched upward from the current directory, or from the file's directory
   with `--file`.
2. `style = "Name"` in `$XDG_CONFIG_HOME/ijfmt/config`, or
   `~/.config/ijfmt/config`.
3. The `--fallback` file, if given.

Names are looked up in `~/.config/ijfmt/styles/`, then in the newest IntelliJ
IDEA configuration's `codestyles/` directory.

When `--fallback` is used because the project has no preferred style, ijfmt also
writes `.idea/codeStyles/codeStyleConfig.xml` so later runs pick it up. It skips
that write in `--check` mode.

### File handling

- Input files must be valid UTF-8. Stdin is decoded lossily.
- CRLF line endings are preserved.
- Changed files are written to a temporary file in the same directory, then
  renamed over the original. An interrupted run can leave `.ijfmt-*.tmp` files.
- On Unix, mode, owner and group are kept. Read-only files, and changed files
  with multiple hard links, are refused. ACLs and extended attributes are not
  copied, and the inode changes; use stdin/stdout if that matters.
- Symlink arguments format their targets. Duplicate paths and aliases are
  processed once.
- Syntax trees deeper than 512 levels are rejected with an error.

## Project layout

```
src/
  lib.rs                 library shared by the CLI, tests and benchmarks
  main.rs                entry point, calls cli::run
  config/                IntelliJ code style XML decoding
  formatter/             formatting pipeline
    parse.rs             tree-sitter parsing
    build/               syntax tree to block tree
      declarations/      classes, methods, parameters, annotations, alignment
      expressions/       arguments, chains, operators, ternaries, grouping
      statements/        blocks, control flow, switch, try, variables
      comments/          comment placement and Javadoc reflow
    flatten/             block tree to leaf sequence
    wrap/                line wrapping decisions
    emit/                text emission and indentation
    lines.rs             merging formatted output into selected lines
  cli/                   binary-only code
    args.rs              command-line arguments
    style.rs             style discovery
    files/               discovery, git changed lines, parallel runs, atomic writes
tests/
  fmt/                   public style XML files
  fixtures/              .input.java / .expected.java pairs
scripts/                 check suite and fixture tooling
benches/                 criterion benchmarks
```

Modules with submodules use `foo/mod.rs`. The pipeline runs parse → build →
flatten → wrap → emit, and repeats wrap and emit until indentation converges.
Wrapping uses indentation feedback from emission, so their update order affects
output. Per-pass state lives in `wrap/scan.rs` and `emit/state.rs`.

## Development

Run the full check suite (needs JDK 17 or newer on `PATH`, or set
`IJFMT_JAVA_HOME`):

```sh
bash scripts/check.sh
```

It runs rustfmt, Clippy, unit and integration tests, benchmark smoke checks,
and a Java test that compares program behaviour before and after formatting.

Quicker loops:

```sh
cargo test --locked --lib --bins                                      # unit tests
cargo test --locked --release --test integration_test                 # all fixtures
cargo test --locked --release --test integration_test -- sample::Foo  # one fixture
```

### Fixtures

Each fixture is an `.input.java` and an `.expected.java` produced by IntelliJ.
`tests/integration_test.rs` creates one test per style and fixture, comparing
ijfmt's output with the expected file byte for byte.

**Never edit fixtures or run ijfmt on them.** The expected files are the source
of truth; fix mismatches in the formatter.

Public styles live in `tests/fmt/`. Private styles and fixtures go in the
git-ignored `tests/fmt/private/` and `tests/fixtures/private/`; both must exist
for the private corpus to load.

`scripts/fixtures.sh` manages fixtures using IntelliJ's command-line formatter:

```sh
scripts/fixtures.sh add ~/src/MyClass.java
scripts/fixtures.sh add-project ~/projects/App/src/main/java --dry-run
scripts/fixtures.sh seed intellij-java-google-style
scripts/fixtures.sh regen [style] [fixture]
```

`scripts/fetch-example-repos.sh [N]` clones the top N starred Java repositories
from GitHub into `example-repos/` (needs `gh`), for wider testing.

### Benchmarks

```sh
cargo bench --bench formatter -- --save-baseline before
# make the change
cargo bench --bench formatter -- --baseline before
```

The benchmark covers small files, deep nesting, chains, Javadoc and large
classes. It measures process CPU time, including worker threads, with input and
style loading outside the timed loop.

| Variable | Effect |
| --- | --- |
| `IJFMT_BENCH_CLOCK=wall` | Measure elapsed time instead (separate baselines) |
| `IJFMT_BENCH_DIR` | Directory of Java files to benchmark instead |
| `IJFMT_BENCH_STYLE` | Style XML to benchmark with |

Keep the compiler, style, inputs and machine the same between runs, with no
competing builds. Results are in `target/criterion/`; check the change estimates
and confidence intervals, since a passing run can still show a regression. Run
the fixture suite afterwards to confirm output is unchanged.
