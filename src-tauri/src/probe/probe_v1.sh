#!/bin/sh
# Nova Rack read-only probe. Emits line-oriented sections; never writes remotely.
LC_ALL=C
export LC_ALL
NONCE="$1"

section() { printf '@@%s\n' "$1"; }

section meta
printf 'os=%s\n' "$(uname -s 2>/dev/null)"
printf 'kernel=%s\n' "$(uname -r 2>/dev/null)"
printf 'arch=%s\n' "$(uname -m 2>/dev/null)"
printf 'hostname=%s\n' "$(hostname 2>/dev/null)"
printf 'distro=%s\n' "$( ( . /etc/os-release 2>/dev/null; printf '%s' "${PRETTY_NAME:-$NAME}" ) 2>/dev/null )"
printf 'uptime_secs=%s\n' "$(cut -d. -f1 /proc/uptime 2>/dev/null)"
printf 'cpu_count=%s\n' "$( (nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0) | head -1 )"
printf 'memory_total_bytes=%s\n' "$(awk '/^MemTotal:/ {printf "%d", $2 * 1024}' /proc/meminfo 2>/dev/null)"
printf 'load1=%s\n' "$(cut -d' ' -f1 /proc/loadavg 2>/dev/null)"
if [ -f /.dockerenv ] || grep -Eq 'docker|lxc|kubepods|containerd' /proc/1/cgroup 2>/dev/null; then
  printf 'containerized=1\n'
else
  printf 'containerized=0\n'
fi

section df
df -P -k 2>/dev/null

section dfi
df -P -i 2>/dev/null

section mounts
cat /proc/self/mounts 2>/dev/null

section lsblk
lsblk -J -b -o NAME,KNAME,TYPE,SIZE,RO,ROTA,FSTYPE,MOUNTPOINTS,MODEL,SERIAL 2>/dev/null \
  || lsblk -J -b -o NAME,KNAME,TYPE,SIZE,RO,ROTA,FSTYPE,MOUNTPOINT,MODEL,SERIAL 2>/dev/null \
  || true

section caps
for c in lsblk journalctl systemctl docker timeout apt-get dnf yum pacman apk; do
  if command -v "$c" >/dev/null 2>&1; then printf '%s=1\n' "$c"; else printf '%s=0\n' "$c"; fi
done
if command -v du >/dev/null 2>&1 && du --help 2>&1 | grep -q -- '-0'; then printf 'du0=1\n'; else printf 'du0=0\n'; fi
if command -v find >/dev/null 2>&1 && find --version 2>/dev/null | grep -q 'GNU'; then printf 'gnufind=1\n'; else printf 'gnufind=0\n'; fi
if command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1; then printf 'sudo_nopass=1\n'; else printf 'sudo_nopass=0\n'; fi
printf 'uid=%s\n' "$(id -u 2>/dev/null)"
printf 'shell=sh\n'

section end
printf '%s\n' "$NONCE"
