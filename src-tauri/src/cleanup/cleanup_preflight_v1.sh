#!/bin/sh
# Nova Rack preflight for controlled cleanup.
# Args: action path expected_size expected_mtime
LC_ALL=C
export LC_ALL
action="$1"
path="$2"
expected_size="$3"
expected_mtime="$4"

fail() {
  printf 'PREFLIGHT=FAIL:%s\n' "$1"
  exit 1
}

case "$action" in
  apt_cache|dnf_cache|yum_cache|pacman_cache|apk_cache|journal_7d|tmp_old)
    if [ "$(id -u)" != "0" ] && ! sudo -n true >/dev/null 2>&1; then
      fail "passwordless sudo required"
    fi
    ;;
esac

case "$action" in
  apt_cache) command -v apt-get >/dev/null 2>&1 || fail "apt-get not found" ;;
  dnf_cache) command -v dnf >/dev/null 2>&1 || fail "dnf not found" ;;
  yum_cache) command -v yum >/dev/null 2>&1 || fail "yum not found" ;;
  pacman_cache) command -v pacman >/dev/null 2>&1 || fail "pacman not found" ;;
  apk_cache) command -v apk >/dev/null 2>&1 || fail "apk not found" ;;
  journal_7d) command -v journalctl >/dev/null 2>&1 || fail "journalctl not found" ;;
  docker_dangling) command -v docker >/dev/null 2>&1 || fail "docker not found" ;;
  npm_cache) command -v npm >/dev/null 2>&1 || fail "npm not found" ;;
  pip_cache) command -v pip >/dev/null 2>&1 || fail "pip not found" ;;
  tmp_old|quarantine|delete_file)
    [ -n "$path" ] || fail "missing path"
    [ -e "$path" ] || fail "path no longer exists"
    [ -L "$path" ] && fail "path is now a symlink"
    if [ "$action" = "delete_file" ] && [ -d "$path" ]; then
      fail "refusing to permanently delete a directory"
    fi
    if command -v stat >/dev/null 2>&1; then
      # Directories report their entry size, while du reports recursive bytes.
      # Only compare fingerprints for regular files.
      if [ -f "$path" ]; then
        actual_size="$(stat -c %s -- "$path" 2>/dev/null)"
        if [ -n "$expected_size" ] && [ "$expected_size" != "0" ] && [ "$actual_size" != "$expected_size" ]; then
          fail "size changed since planning"
        fi
        if [ -n "$expected_mtime" ] && [ "$expected_mtime" != "0" ]; then
          actual_mtime="$(stat -c %Y -- "$path" 2>/dev/null)"
          if [ "$actual_mtime" != "$expected_mtime" ]; then
            fail "mtime changed since planning"
          fi
        fi
      fi
    fi
    if command -v findmnt >/dev/null 2>&1; then
      opts="$(findmnt -no OPTIONS --target "$path" 2>/dev/null)"
      case ",$opts," in
        *,ro,*) fail "filesystem is mounted read-only" ;;
      esac
    fi
    ;;
  *)
    fail "unknown action"
    ;;
esac

printf 'PREFLIGHT=OK\n'
