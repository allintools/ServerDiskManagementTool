#!/bin/sh
# Nova Rack read-only reclaim candidate detection.
LC_ALL=C
export LC_ALL

emit() {
  id="$1"; path="$2"
  if [ -e "$path" ]; then
    b="$(du -sb "$path" 2>/dev/null | cut -f1)"
    case "$b" in ''|*[!0-9]*) b=0 ;; esac
    printf '%s|%s|%s\n' "$id" "$b" "$path"
  fi
}

emit apt_cache /var/cache/apt/archives
emit dnf_cache /var/cache/dnf
emit yum_cache /var/cache/yum
emit pacman_cache /var/cache/pacman/pkg
emit apk_cache /var/cache/apk
emit journal_7d /var/log/journal
emit npm_cache "$HOME/.npm"
emit pip_cache "$HOME/.cache/pip"

if command -v docker >/dev/null 2>&1; then
  printf 'docker_dangling|0|docker\n'
fi

tmp_bytes="$(find /tmp -xdev -type f -mtime +7 -printf '%s\n' 2>/dev/null | awk '{s+=$1} END {printf "%d", s+0}')"
case "$tmp_bytes" in ''|*[!0-9]*) tmp_bytes=0 ;; esac
if [ "$tmp_bytes" -gt 0 ]; then
  printf 'tmp_old|%s|/tmp\n' "$tmp_bytes"
fi
