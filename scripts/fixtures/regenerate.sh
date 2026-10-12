#!/usr/bin/env bash

cmd_regen() (
    local style_filter="${1:-}"
    local fixture_filter="${2:-}"

    local -a style_xmls
    load_styles style_xmls
    local total_failed=0

    for xml in "${style_xmls[@]}"; do
        local sname
        sname="$(style_name "$xml")"

        if [[ -n "$style_filter" && "$sname" != "$style_filter" ]]; then
            continue
        fi

        local fixture_dir="$FIXTURES_PRIVATE/$sname"
        [[ -d "$fixture_dir" ]] || { echo "  Skipping $sname (no fixture dir)"; continue; }

        echo "=== Regenerating: $sname ==="

        if [[ -n "$fixture_filter" ]]; then
            local input_file="$fixture_dir/${fixture_filter}.input.java"
            [[ -f "$input_file" ]] || { echo "  Fixture not found: $fixture_filter"; continue; }

            local tmpfile
            tmpfile="$(mktemp --suffix=.java)"
            trap 'rm -f -- "$tmpfile"' EXIT
            cp "$input_file" "$tmpfile"
            ij_fmt "$xml" "$tmpfile"
            [[ -f "$tmpfile" ]] || die "formatter did not produce output for $input_file"
            cp "$tmpfile" "$fixture_dir/${fixture_filter}.expected.java"
            rm -f "$tmpfile"

            echo "  [OK] $fixture_filter"
            echo "  Done: 1 regenerated."
            echo ""
            continue
        fi

        local tmpdir
        tmpdir="$(mktemp -d)"
        trap 'rm -rf -- "$tmpdir"' EXIT

        local -a batch_names=()
        local count=0

        for input_file in "$fixture_dir"/*.input.java; do
            [[ -f "$input_file" ]] || continue

            local name
            name="$(basename "$input_file" .input.java)"

            cp "$input_file" "$tmpdir/${name}.java"
            batch_names+=("$name")
        done

        if [[ ${#batch_names[@]} -gt 0 ]]; then
            echo "  Formatting ${#batch_names[@]} files in one batch..."
            ij_fmt "$xml" -r "$tmpdir"

            for name in "${batch_names[@]}"; do
                local tmpfmt="$tmpdir/${name}.java"
                if [[ -f "$tmpfmt" ]]; then
                    cp "$tmpfmt" "$fixture_dir/${name}.expected.java"
                    (( count++ )) || true
                else
                    echo "  [FAIL] $name"
                    (( total_failed++ )) || true
                fi
            done
        fi

        rm -rf "$tmpdir"

        echo "  Done: $count regenerated."
        echo ""
    done

    echo "=== All done ==="
    [[ $total_failed -eq 0 ]]
)
