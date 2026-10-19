<script setup lang="ts">
import { computed } from 'vue'
import {
  Activity, AlertTriangle, ArrowUpRight, BarChart3, Boxes, Check, ChevronRight, CircleHelp,
  Clock3, Cpu, Database, Download, ExternalLink, FileText, Folder, Gauge, HardDrive, KeyRound,
  LayoutDashboard, ListChecks, LoaderCircle, Lock, MemoryStick, Network, Package, Play, Plus,
  RefreshCw, RotateCcw, ScanLine, Search, Server, Settings, ShieldCheck, SlidersHorizontal,
  Square, Terminal, Trash2, Undo2, Upload, Wifi, X, Moon, Sun,
} from 'lucide-vue-next'

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 18 })

const icons: Record<string, unknown> = {
  fleet: LayoutDashboard, servers: Server, server: Server, analysis: BarChart3, tasks: ListChecks,
  alerts: AlertTriangle, settings: Settings, plus: Plus, refresh: RefreshCw, search: Search,
  right: ChevronRight, chevron: ChevronRight, disk: HardDrive, hardDrive: HardDrive,
  cpu: Cpu, memory: MemoryStick, terminal: Terminal, shield: ShieldCheck, check: Check,
  warning: AlertTriangle, x: X, external: ExternalLink, folder: Folder, file: FileText,
  trash: Trash2, undo: Undo2, clock: Clock3, play: Play, stop: Square, activity: Activity,
  database: Database, network: Network, key: KeyRound, download: Download, upload: Upload,
  layers: Boxes, chart: Gauge, filter: SlidersHorizontal, lock: Lock, package: Package,
  scan: ScanLine, help: CircleHelp, loader: LoaderCircle, arrowUpRight: ArrowUpRight,
  wifi: Wifi, rotate: RotateCcw, moon: Moon, sun: Sun,
}

const component = computed(() => icons[props.name] ?? CircleHelp)
</script>

<template>
  <component :is="component" :size="size" :stroke-width="1.8" aria-hidden="true" />
</template>
