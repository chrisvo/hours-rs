#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

date_to_check="${1:?Usage: scripts/verify-offline.sh YYYY-MM-DD}"
binary="$PWD/dist/Hours.app/Contents/MacOS/hours-rs"
if [[ ! -x "$binary" ]]; then
    printf 'Build the app with scripts/bundle-macos.sh first.\n' >&2
    exit 1
fi

verification_dir=$(mktemp -d "${TMPDIR:-/tmp}/hours-offline.XXXXXX")
policy='(version 1)(allow default)(deny network*)'
sandbox-exec -p "$policy" "$binary" export "$verification_dir/library.hours.json"
HOURS_RS_HOME="$verification_dir/restored" sandbox-exec -p "$policy" \
    "$binary" import "$verification_dir/library.hours.json"
for hour in readings morning daytime evening night; do
    sandbox-exec -p "$policy" "$binary" read "$date_to_check" "$hour" > "$verification_dir/$hour.original.txt"
    HOURS_RS_HOME="$verification_dir/restored" sandbox-exec -p "$policy" \
        "$binary" read "$date_to_check" "$hour" > "$verification_dir/$hour.restored.txt"
    cmp "$verification_dir/$hour.original.txt" "$verification_dir/$hour.restored.txt"
    test -s "$verification_dir/$hour.restored.txt"
done
HOURS_RS_HOME="$verification_dir/restored" sandbox-exec -p "$policy" \
    "$binary" --offline --date "$date_to_check" --hour evening \
    --screenshot "$verification_dir/native-offline.png"
test -s "$verification_dir/native-offline.png"
printf 'Passed: all five offices survived export/import and native rendering with network access denied.\n'
printf 'Local verification artifacts: %s\n' "$verification_dir"
