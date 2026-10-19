#!/bin/sh
# Nova Rack directory usage (read-only). Args: root depth
LC_ALL=C
export LC_ALL
root="$1"
depth="${2:-1}"
if command -v du >/dev/null 2>&1; then
  if du --help 2>&1 | grep -q -- '-0'; then
    printf 'MODE=0\n'
    du -x -B1 -d "$depth" -0 -- "$root" 2>/dev/null
  else
    printf 'MODE=1\n'
    du -x -k -d "$depth" -- "$root" 2>/dev/null
  fi
else
  printf 'MODE=2\n'
fi
