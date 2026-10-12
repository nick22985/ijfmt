#!/usr/bin/env bash

set -euo pipefail

IDEA_FMT="/usr/share/idea/bin/format.sh"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FMT_PRIVATE="$REPO_ROOT/tests/fmt/private"
FIXTURES_PRIVATE="$REPO_ROOT/tests/fixtures/private"

source "$SCRIPT_DIR/fixtures/common.sh"
source "$SCRIPT_DIR/fixtures/add.sh"
source "$SCRIPT_DIR/fixtures/add_project.sh"
source "$SCRIPT_DIR/fixtures/seed.sh"
source "$SCRIPT_DIR/fixtures/regenerate.sh"
source "$SCRIPT_DIR/fixtures/format.sh"

usage() {
    cat >&2 <<'EOF'
Usage: fixtures.sh <command> [args...]

Commands:
  add <source.java>              Add a single file as a fixture for all private styles
  add-project <source_dir>       Add all .java files from a directory
  seed <style>                   Populate a style's fixtures from other styles
  regen [style] [fixture]        Regenerate .expected.java from .input.java
  fmt <input.java> <style.xml>   Format a single fixture with a specific style

Options (add / add-project / seed):
  --dry-run       Preview without writing
  --force         Overwrite existing fixtures
  --prefix-dirs   Prefix fixture names with top-level subdir (add-project only)

Examples:
  fixtures.sh add ~/src/MyClass.java
  fixtures.sh add-project ~/projects/ExampleApp/src/main/java
  fixtures.sh add-project ~/projects/ExampleApp/src/main/java --dry-run
  fixtures.sh seed intellij-java-google-style
  fixtures.sh regen
  fixtures.sh regen custom
  fixtures.sh regen custom UserManager
  fixtures.sh fmt tests/fixtures/private/custom/Foo.input.java tests/fmt/private/Custom.xml
EOF
    exit 1
}

main() {
    local command="${1:-}"
    shift || true

    case "$command" in
        add)         cmd_add "$@" ;;
        add-project) cmd_add_project "$@" ;;
        seed)        cmd_seed "$@" ;;
        regen)       cmd_regen "$@" ;;
        fmt)         cmd_fmt "$@" ;;
        -h|--help|help|"") usage ;;
        *)           echo "Unknown command: $command" >&2; usage ;;
    esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
