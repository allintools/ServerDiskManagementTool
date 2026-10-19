#!/bin/sh
# Nova Rack controlled cleanup actions. No user-supplied shell fragments.
# Args: action path quarantine_path
LC_ALL=C
export LC_ALL
action="$1"
path="$2"
qpath="$3"

as_root() {
  if [ "$(id -u)" = 0 ]; then
    "$@"
  else
    sudo -n "$@"
  fi
}

try_root() {
  "$@" 2>/dev/null && return 0
  as_root "$@"
}

free_kb() {
  p="$1"
  [ -n "$p" ] || p=/
  df -P -k "$p" 2>/dev/null | awk 'NR==2 {gsub(/[^0-9]/,"",$4); print $4}'
}

probe_path="/"
if [ -n "$path" ]; then
  if [ -d "$path" ]; then
    probe_path="$path"
  else
    probe_path="$(dirname "$path" 2>/dev/null)"
    [ -n "$probe_path" ] || probe_path=/
  fi
fi
before="$(free_kb "$probe_path")"
[ -n "$before" ] || before=0

case "$action" in
  apt_cache) as_root apt-get clean ;;
  dnf_cache) as_root dnf clean all ;;
  yum_cache) as_root yum clean all ;;
  pacman_cache) as_root pacman -Sc --noconfirm ;;
  apk_cache) as_root apk cache clean ;;
  journal_7d) as_root journalctl --vacuum-time=7d ;;
  docker_dangling)
    if docker info >/dev/null 2>&1; then
      docker image prune -f && docker builder prune -f
    else
      as_root docker image prune -f && as_root docker builder prune -f
    fi
    ;;
  npm_cache) npm cache clean --force ;;
  pip_cache) pip cache purge ;;
  tmp_old) as_root find /tmp -xdev -type f -mtime +7 -delete ;;
  quarantine)
    [ -n "$path" ] && [ -n "$qpath" ] || { printf 'CLEANUP=FAIL:missing path\n'; exit 1; }
    try_root mkdir -p "$(dirname "$qpath")" || { printf 'CLEANUP=FAIL:mkdir quarantine\n'; exit 1; }
    try_root mv -- "$path" "$qpath" || { printf 'CLEANUP=FAIL:move\n'; exit 1; }
    ;;
  delete_file)
    [ -n "$path" ] || { printf 'CLEANUP=FAIL:missing path\n'; exit 1; }
    try_root rm -f -- "$path" || { printf 'CLEANUP=FAIL:delete\n'; exit 1; }
    ;;
  restore)
    [ -n "$path" ] && [ -n "$qpath" ] || { printf 'CLEANUP=FAIL:missing path\n'; exit 1; }
    try_root mkdir -p "$(dirname "$path")" || { printf 'CLEANUP=FAIL:mkdir restore\n'; exit 1; }
    try_root mv -- "$qpath" "$path" || { printf 'CLEANUP=FAIL:restore\n'; exit 1; }
    ;;
  purge)
    [ -n "$qpath" ] || { printf 'CLEANUP=FAIL:missing quarantine\n'; exit 1; }
    try_root rm -rf -- "$qpath" || { printf 'CLEANUP=FAIL:purge\n'; exit 1; }
    ;;
  *)
    printf 'CLEANUP=FAIL:unknown action\n'
    exit 1
    ;;
esac
code=$?
after="$(free_kb "$probe_path")"
[ -n "$after" ] || after=0
printf 'FREE_BEFORE=%s\n' "$before"
printf 'FREE_AFTER=%s\n' "$after"
printf 'CLEANUP=%s\n' "$([ "$code" -eq 0 ] && echo OK || echo FAIL)"
exit "$code"
