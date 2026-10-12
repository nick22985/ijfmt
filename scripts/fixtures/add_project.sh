#!/usr/bin/env bash

cmd_add_project() (
    local dry_run=false force=false prefix_dirs=false
    local batch_size=5000
    local positional=()

    for arg in "$@"; do
        case "$arg" in
            --dry-run)     dry_run=true ;;
            --force)       force=true ;;
            --prefix-dirs) prefix_dirs=true ;;
            --batch-size=*) batch_size="${arg#*=}" ;;
            *)             positional+=("$arg") ;;
        esac
    done

    if [[ ${#positional[@]} -ne 1 ]]; then
        echo "Usage: $0 add-project <source_dir> [--dry-run] [--force] [--prefix-dirs] [--batch-size=N]" >&2
        exit 1
    fi

    [[ "$batch_size" =~ ^[1-9][0-9]*$ && ${#batch_size} -le 10 ]] && ((batch_size <= 2147483647)) || die "batch size must be an integer from 1 to 2147483647"

    local source_dir
    source_dir="$(cd "${positional[0]}" && pwd)"
    [[ -d "$source_dir" ]] || die "source directory not found: ${positional[0]}"

    local start_time=$SECONDS

    echo "  Scanning for .java files..."
    local scan_file
    scan_file="$(mktemp)"
    trap 'rm -f -- "$scan_file"' EXIT
    find "$source_dir" -name '*.java' -type f -print0 > "$scan_file"
    sort -z "$scan_file" -o "$scan_file"
    mapfile -d '' -t java_files < "$scan_file"
    rm -f -- "$scan_file"
    trap - EXIT
    [[ ${#java_files[@]} -gt 0 ]] || die "no .java files found in $source_dir"
    echo "  Found ${#java_files[@]} files in $(( SECONDS - start_time ))s"

    local -a style_xmls
    load_styles style_xmls

    echo "=== Adding project fixtures ==="
    echo "Source:     $source_dir"
    echo "Files:      ${#java_files[@]} .java files"
    echo "Styles:     $(printf '%s ' "${style_xmls[@]}" | sed "s|$FMT_PRIVATE/||g; s|\.xml||g")"
    echo "Batch size: $batch_size"
    echo ""

    local total_added=0 total_skipped=0 total_failed=0
    local total_files=${#java_files[@]}
    local style_num=0

    for xml in "${style_xmls[@]}"; do
        (( style_num++ )) || true
        local sname
        sname="$(style_name "$xml")"
        local fixture_dir="$FIXTURES_PRIVATE/$sname"
        local style_start=$SECONDS

        echo "============================================"
        echo "[$style_num/${#style_xmls[@]}] Style: $sname"
        echo "============================================"

        local added=0 skipped=0 failed=0
        local -A seen_names=()

        if $dry_run; then
            local i=0
            for java_file in "${java_files[@]}"; do
                (( i++ )) || true
                local name
                fixture_name name "$java_file" "$source_dir" "$prefix_dirs"

                if [[ -n "${seen_names[$name]+x}" ]]; then
                    (( skipped++ )) || true
                    continue
                fi
                seen_names[$name]=1

                local input_file="$fixture_dir/${name}.input.java"

                if pair_exists "$input_file" && ! $force; then
                    (( skipped++ )) || true
                else
                    (( added++ )) || true
                fi

                if (( i % 10000 == 0 )); then
                    printf "  [dry-run] Scanned %d/%d (%.1f%%) — %d add, %d skip\n" \
                        "$i" "$total_files" "$(percentage "$i" "$total_files")" "$added" "$skipped"
                fi
            done
            printf "  [dry-run] Scanned %d/%d — %d would add, %d would skip\n" \
                "$total_files" "$total_files" "$added" "$skipped"
        else
            local -a batch_names=()
            local -a batch_sources=()
            local scan_count=0

            echo "  Collecting files to format..."
            for java_file in "${java_files[@]}"; do
                (( scan_count++ )) || true
                local name
                fixture_name name "$java_file" "$source_dir" "$prefix_dirs"

                if [[ -n "${seen_names[$name]+x}" ]]; then
                    (( skipped++ )) || true
                    continue
                fi
                seen_names[$name]=1

                local input_file="$fixture_dir/${name}.input.java"

                if pair_exists "$input_file" && ! $force; then
                    (( skipped++ )) || true
                    continue
                fi

                batch_names+=("$name")
                batch_sources+=("$java_file")

                if (( scan_count % 50000 == 0 )); then
                    printf "  Scanned %d/%d (%.1f%%) — %d to format, %d skip\n" \
                        "$scan_count" "$total_files" "$(percentage "$scan_count" "$total_files")" \
                        "${#batch_names[@]}" "$skipped"
                fi
            done

            local to_format=${#batch_names[@]}
            echo "  Scan done: $to_format to format, $skipped skipped (${scan_count} scanned in $(( SECONDS - style_start ))s)"

            if [[ $to_format -gt 0 ]]; then
                local num_batches=$(( (to_format + batch_size - 1) / batch_size ))
                echo "  Processing $to_format files in $num_batches batches of up to $batch_size..."
                echo ""

                local batch_num=0
                local processed=0

                for (( offset=0; offset < to_format; offset += batch_size )); do
                    (( batch_num++ )) || true
                    local end=$((offset + batch_size))
                    (( end > to_format )) && end=$to_format
                    local this_batch_size=$((end - offset))

                    local batch_start=$SECONDS
                    local tmpdir
                    tmpdir="$(mktemp -d)"
                    trap 'rm -rf -- "$tmpdir"' EXIT

                    printf "  Batch %d/%d: copying %d files to tmpdir..." "$batch_num" "$num_batches" "$this_batch_size"
                    for (( i=offset; i < end; i++ )); do
                        cp "${batch_sources[$i]}" "$tmpdir/${batch_names[$i]}.java"
                    done
                    local copy_elapsed=$(( SECONDS - batch_start ))
                    echo " done (${copy_elapsed}s)"

                    local fmt_start=$SECONDS
                    printf "  Batch %d/%d: running IntelliJ formatter...\n" "$batch_num" "$num_batches"
                    ij_fmt "$xml" -r "$tmpdir"
                    local fmt_elapsed=$(( SECONDS - fmt_start ))
                    echo "  Batch $batch_num/$num_batches: formatting done (${fmt_elapsed}s)"

                    printf "  Batch %d/%d: saving results..." "$batch_num" "$num_batches"
                    mkdir -p "$fixture_dir"
                    local batch_added=0 batch_failed=0
                    for (( i=offset; i < end; i++ )); do
                        local name="${batch_names[$i]}"
                        local java_file="${batch_sources[$i]}"
                        local tmpfmt="$tmpdir/${name}.java"

                        if [[ -f "$tmpfmt" ]]; then
                            cp "$java_file" "$fixture_dir/${name}.input.java"
                            cp "$tmpfmt" "$fixture_dir/${name}.expected.java"
                            (( batch_added++ )) || true
                        else
                            (( batch_failed++ )) || true
                        fi
                    done
                    echo " done"

                    (( added += batch_added )) || true
                    (( failed += batch_failed )) || true
                    (( processed += this_batch_size )) || true
                    rm -rf "$tmpdir"

                    local batch_elapsed=$(( SECONDS - batch_start ))
                    local total_elapsed=$(( SECONDS - style_start ))
                    local rate=$(( this_batch_size / (batch_elapsed > 0 ? batch_elapsed : 1) ))
                    local remaining_files=$(( to_format - processed ))
                    local eta="?"
                    if (( rate > 0 )); then
                        eta="$(( remaining_files / rate ))s"
                    fi

                    printf "  Batch %d/%d complete: +%d ok, %d fail | %d/%d total (%.1f%%) | %d files/s | ETA %s\n\n" \
                        "$batch_num" "$num_batches" "$batch_added" "$batch_failed" \
                        "$processed" "$to_format" "$(percentage "$processed" "$to_format")" \
                        "$rate" "$eta"
                done
            fi
        fi

        local style_elapsed=$(( SECONDS - style_start ))
        echo "  Style $sname done in ${style_elapsed}s: $added added, $skipped skipped, $failed failed"
        echo ""

        (( total_added += added )) || true
        (( total_skipped += skipped )) || true
        (( total_failed += failed )) || true
    done

    local total_elapsed=$(( SECONDS - start_time ))
    echo "=== Done in ${total_elapsed}s: $total_added added, $total_skipped skipped, $total_failed failed ==="
    [[ $total_added -gt 0 ]] && ! $dry_run && echo "Run tests with:  cargo test"
    [[ $total_failed -eq 0 ]]
)
