#!/usr/bin/env bash
set -euo pipefail

max_lines="${BACKEND_MAX_FILE_LINES:-800}"
failed=0

while IFS= read -r -d '' file; do
  line_count="$(wc -l < "$file" | tr -d ' ')"
  if [ "$line_count" -gt "$max_lines" ]; then
    printf '%s has %s lines; backend files must stay at or below %s lines.\n' \
      "$file" "$line_count" "$max_lines" >&2
    failed=1
  fi
done < <(find backend/src -type f -name '*.rs' -print0 | sort -z)

exit "$failed"
