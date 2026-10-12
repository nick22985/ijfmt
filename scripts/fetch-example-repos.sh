#!/usr/bin/env bash
set -euo pipefail

DEST="./example-repos"
MAX_REPOS="${1:-1000}"
PER_PAGE=50
PAGES=$(( (MAX_REPOS + PER_PAGE - 1) / PER_PAGE ))

mkdir -p "$DEST"

fetched=0

for page in $(seq 1 "$PAGES"); do
  remaining=$((MAX_REPOS - fetched))
  count=$((remaining < PER_PAGE ? remaining : PER_PAGE))

  echo "Fetching page $page (top $count repos)..."

  repos=$(gh api "search/repositories?q=language:java+stars:>1000&sort=stars&order=desc&per_page=$count&page=$page" \
    --jq '.items[].full_name')

  for repo in $repos; do
    dir="$DEST/$(basename "$repo")"
    if [ -d "$dir" ]; then
      echo "  SKIP $repo (already exists)"
    else
      echo "  CLONE $repo -> $dir"
      git clone --depth 1 --filter=blob:limit=1m --no-checkout "https://github.com/$repo.git" "$dir"
      cd "$dir"
      git sparse-checkout init --no-cone
      git sparse-checkout set '*.java'
      git checkout
      cd - > /dev/null
    fi
    fetched=$((fetched + 1))
  done
done

total_files=$(find "$DEST" -name '*.java' | wc -l)
echo ""
echo "Done. $fetched repos, $total_files .java files in $DEST"
