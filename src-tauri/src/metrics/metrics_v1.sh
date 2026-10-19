#!/bin/sh
# Nova Rack one-shot live metrics sample (two reads, ~1s apart). Read-only.
LC_ALL=C
export LC_ALL
snapshot() {
  awk '/^cpu / {print "CPU", $2, $3, $4, $5, $6, $7, $8, $9}' /proc/stat 2>/dev/null
  awk '/^MemTotal:/ {print "MEMTOTAL", $2}' /proc/meminfo 2>/dev/null
  awk '/^MemAvailable:/ {print "MEMAVAIL", $2}' /proc/meminfo 2>/dev/null
  awk 'NR==1 {print "LOAD", $1}' /proc/loadavg 2>/dev/null
  awk '$3 ~ /^(sd|nvme|vd|xvd|md|dm-)/ {print "DISK", $3, $6, $10}' /proc/diskstats 2>/dev/null
}
printf 'SAMPLE=1\n'
snapshot
sleep 1
printf 'SAMPLE=2\n'
snapshot
