#!/bin/sh
# Nova Rack large file scan (read-only). Args: root min_bytes
LC_ALL=C
export LC_ALL
root="$1"
min="$2"
if command -v find >/dev/null 2>&1 && find --version 2>/dev/null | grep -q 'GNU'; then
  find "$root" -xdev -type f -size +"${min}c" -printf '%s\t%T@\t%p\n' 2>/dev/null | head -n 500
fi
