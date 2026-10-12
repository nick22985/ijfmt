#!/usr/bin/env bash

cmd_seed() (
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
        echo "Usage: $0 seed <style> [--dry-run] [--force]" >&2
        exit 1
    fi

    local target_style="${positional[0]}"

    local target_xml=""
    local -a style_xmls
    load_styles style_xmls
    for xml in "${style_xmls[@]}"; do
        if [[ "$(style_name "$xml")" == "$target_style" ]]; then
            target_xml="$xml"
            break
        fi
    done

    [[ -n "$target_xml" ]] || die "no style XML found matching '$target_style' in $FMT_PRIVATE"

    local target_dir="$FIXTURES_PRIVATE/$target_style"

    local -A input_files=()
    for dir in "$FIXTURES_PRIVATE"/*/; do
        [[ -d "$dir" ]] || continue
        local dirname
        dirname="$(basename "$dir")"
        [[ "$dirname" != "$target_style" ]] || continue

        for input_file in "$dir"*.input.java; do
            [[ -f "$input_file" ]] || continue
            local name
            name="$(basename "$input_file" .input.java)"
            if [[ -z "${input_files[$name]+x}" ]]; then
                input_files[$name]="$input_file"
            fi
        done
    done

    [[ ${#input_files[@]} -gt 0 ]] || die "no .input.java files found in other private fixture dirs"

    echo "=== Seeding fixtures for: $target_style ==="
    echo "Style XML: $target_xml"
    echo "Target:    $target_dir"
    echo "Sources:   ${#input_files[@]} unique input files from other styles"
    echo ""

    local added=0 skipped=0 failed=0

    local -a sorted_names
    mapfile -d '' -t sorted_names < <(printf '%s\0' "${!input_files[@]}" | sort -z)
    if $dry_run; then
        for name in "${sorted_names[@]}"; do
            local dest="$target_dir/${name}.input.java"
            local source="${input_files[$name]}"
            local source_style
            source_style="$(basename "$(dirname "$source")")"

            if pair_exists "$dest" && ! $force; then
                echo "  [SKIP]      $name (already exists)"
                (( skipped++ )) || true
            elif [[ -f "$dest" ]]; then
                echo "  [OVERWRITE] $name (from $source_style)"
                (( added++ )) || true
            else
                echo "  [ADD]       $name (from $source_style)"
                (( added++ )) || true
            fi
        done
    else
        mkdir -p "$target_dir"

        local tmpdir
        tmpdir="$(mktemp -d)"
        trap 'rm -rf -- "$tmpdir"' EXIT

        local -a batch_names=()

        for name in "${sorted_names[@]}"; do
            local dest="$target_dir/${name}.input.java"
            local source="${input_files[$name]}"

            if pair_exists "$dest" && ! $force; then
                (( skipped++ )) || true
                continue
            fi

            cp "$source" "$tmpdir/${name}.java"
            batch_names+=("$name")
        done

        if [[ ${#batch_names[@]} -gt 0 ]]; then
            echo "  Formatting ${#batch_names[@]} files in one batch..."
            ij_fmt "$target_xml" -r "$tmpdir"

            for name in "${batch_names[@]}"; do
                local tmpfmt="$tmpdir/${name}.java"
                if [[ -f "$tmpfmt" ]]; then
                    cp "${input_files[$name]}" "$target_dir/${name}.input.java"
                    cp "$tmpfmt" "$target_dir/${name}.expected.java"
                    echo "  [OK]   $name"
                    (( added++ )) || true
                else
                    echo "  [FAIL] $name"
                    (( failed++ )) || true
                fi
            done
        fi

        rm -rf "$tmpdir"
    fi

    echo ""
    echo "=== Done: $added added, $skipped skipped, $failed failed ==="
    [[ $added -gt 0 ]] && ! $dry_run && echo "Run tests with:  cargo test"
    [[ $failed -eq 0 ]]
)
