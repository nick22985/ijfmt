#!/usr/bin/env bash

cmd_fmt() (
    if [[ $# -ne 2 ]]; then
        echo "Usage: $0 fmt <input_file> <style_xml>" >&2
        exit 1
    fi

    local input="$1" style="$2"

    [[ -f "$input" ]] || die "input file not found: $input"
    [[ -f "$style" ]] || die "style XML not found: $style"
    [[ "$input" == *.input.java ]] || die "input file must end in .input.java (got: $input)"

    local expected="${input%.input.java}.expected.java"
    local tmpfile
    tmpfile="$(mktemp --suffix=.java)"
    trap 'rm -f -- "$tmpfile"' EXIT

    cp "$input" "$tmpfile"

    echo "Formatting: $input"
    echo "Style:      $style"
    echo "Output:     $expected"

    ij_fmt "$style" "$tmpfile"
    [[ -f "$tmpfile" ]] || die "formatter did not produce output for $input"
    cp "$tmpfile" "$expected"
    echo "Done."
)
