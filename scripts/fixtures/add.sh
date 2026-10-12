#!/usr/bin/env bash

cmd_add() (
    local dry_run=false force=false
    local positional=()

    for arg in "$@"; do
        case "$arg" in
            --dry-run) dry_run=true ;;
            --force)   force=true ;;
            *)         positional+=("$arg") ;;
        esac
    done

    if [[ ${#positional[@]} -ne 1 ]]; then
        echo "Usage: $0 add <source.java> [--dry-run] [--force]" >&2
        exit 1
    fi

    local source="${positional[0]}"
    [[ -f "$source" ]] || die "source file not found: $source"
    local -a style_xmls
    load_styles style_xmls

    local name
    name="$(basename "$source" .java)"

    echo "=== Adding fixture: $name ==="
    echo "Source: $source"
    echo ""

    local total_added=0 total_skipped=0

    for xml in "${style_xmls[@]}"; do
        local sname
        sname="$(style_name "$xml")"
        local fixture_dir="$FIXTURES_PRIVATE/$sname"
        local input_file="$fixture_dir/${name}.input.java"
        local expected_file="$fixture_dir/${name}.expected.java"

        echo "[ $sname ]"

        if pair_exists "$input_file" && ! $force; then
            echo "  [SKIP] $name (already exists)"
            (( total_skipped++ )) || true
            continue
        fi

        if $dry_run; then
            if [[ -f "$input_file" ]]; then
                echo "  [OVERWRITE] $name"
            else
                echo "  [ADD]  $name"
            fi
            (( total_added++ )) || true
            continue
        fi

        mkdir -p "$fixture_dir"
        local tmpfile
        tmpfile="$(mktemp --suffix=.java)"
        trap 'rm -f -- "$tmpfile"' EXIT
        cp "$source" "$tmpfile"
        ij_fmt "$xml" "$tmpfile"
        [[ -f "$tmpfile" ]] || die "formatter did not produce output for $source"
        cp "$source" "$input_file"
        cp "$tmpfile" "$expected_file"
        rm -f "$tmpfile"

        echo "  [OK]   $name"
        (( total_added++ )) || true
    done

    echo ""
    echo "=== Done: $total_added added, $total_skipped skipped ==="
)
