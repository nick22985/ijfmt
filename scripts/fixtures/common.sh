#!/usr/bin/env bash

ij_fmt() {
    local style="$1"
    shift
    IDEA_NO_LAUNCHER=1 "$IDEA_FMT" -s "$style" "$@" >&2
}

die() { echo "Error: $*" >&2; exit 1; }

load_styles() {
    local -n result="$1"
    result=()
    [[ -d "$FMT_PRIVATE" && -r "$FMT_PRIVATE" && -x "$FMT_PRIVATE" ]] || die "cannot read styles in $FMT_PRIVATE"
    local xml
    for xml in "$FMT_PRIVATE"/*.xml; do
        [[ -f "$xml" ]] && result+=("$xml")
    done
    [[ ${#result[@]} -gt 0 ]] || die "no style XMLs found in $FMT_PRIVATE"
}

style_name() {
    basename "$1" .xml | tr '[:upper:]' '[:lower:]'
}

pair_exists() {
    [[ -f "$1" && -f "${1%.input.java}.expected.java" ]]
}

fixture_name() {
    local -n result="$1"
    local path="$2" root="$3" prefix="$4"
    if $prefix; then
        path="${path#"$root/"}"
        path="${path%.java}"
        result="${path//\//__}"
    else
        path="${path##*/}"
        result="${path%.java}"
    fi
}

percentage() {
    local tenths=$(( $1 * 1000 / $2 ))
    printf '%d.%d' "$((tenths / 10))" "$((tenths % 10))"
}
