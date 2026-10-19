<script setup lang="ts">
import { computed } from 'vue'
import type { UsageNode } from '../types'
import { formatBytes, formatPercent } from '../services/format'
import { appSettings } from '../services/store'

const props = defineProps<{ nodes: UsageNode[]; total?: number; loading?: boolean }>()
const emit = defineEmits<{ (e: 'drill', node: UsageNode): void }>()

interface Rect { node: UsageNode; x: number; y: number; w: number; h: number; pct: number }

const W = 1000
const H = 460
const darkPalette = ['#4fd1c5', '#6ea8fe', '#a78bfa', '#f2b84b', '#f16a6a', '#5eead4', '#93c5fd', '#f9a8d4', '#86efac', '#fcd34d']
const lightPalette = ['#18181b', '#27272a', '#3f3f46', '#52525b', '#71717a', '#a1a1aa', '#d4d4d8', '#e4e4e7', '#a1a1aa', '#71717a']
const palette = computed(() => appSettings.theme === 'light' ? lightPalette : darkPalette)

function layout(nodes: UsageNode[], total: number): Rect[] {
  const sorted = nodes.filter(n => n.sizeBytes > 0).sort((a, b) => b.sizeBytes - a.sizeBytes).slice(0, 60)
  if (!sorted.length) return []
  const sum = sorted.reduce((acc, n) => acc + n.sizeBytes, 0) || 1
  const rects: Rect[] = []
  function split(items: UsageNode[], x: number, y: number, w: number, h: number, areaTotal: number) {
    if (!items.length) return
    if (items.length === 1) {
      const n = items[0]
      rects.push({ node: n, x, y, w, h, pct: n.sizeBytes / total * 100 })
      return
    }
    const target = areaTotal / 2
    let acc = 0
    let index = 0
    for (; index < items.length - 1; index++) {
      if (acc + items[index].sizeBytes > target) break
      acc += items[index].sizeBytes
    }
    if (index === 0) index = 1
    const left = items.slice(0, index)
    const right = items.slice(index)
    const leftArea = left.reduce((s, n) => s + n.sizeBytes, 0)
    const ratio = leftArea / areaTotal
    if (w >= h) {
      const lw = w * ratio
      split(left, x, y, lw, h, leftArea)
      split(right, x + lw, y, w - lw, h, areaTotal - leftArea)
    } else {
      const lh = h * ratio
      split(left, x, y, w, lh, leftArea)
      split(right, x, y + lh, w, h - lh, areaTotal - leftArea)
    }
  }
  split(sorted, 0, 0, W, H, sum)
  return rects
}

const rects = computed(() => layout(props.nodes, props.total || props.nodes.reduce((s, n) => s + n.sizeBytes, 0) || 1))
function fill(index: number) { return palette.value[index % palette.value.length] }
function labelVisible(r: Rect) { return r.w > 90 && r.h > 34 }
function drill(node: UsageNode) { if (!props.loading) emit('drill', node) }
</script>

<template>
  <div class="treemap-shell">
    <svg class="treemap" :viewBox="`0 0 ${W} ${H}`" role="img" aria-label="目录容量 Treemap">
      <g v-for="(rect, index) in rects" :key="rect.node.id">
        <rect :x="rect.x + 2" :y="rect.y + 2" :width="Math.max(1, rect.w - 4)" :height="Math.max(1, rect.h - 4)" rx="7" :fill="fill(index)" :fill-opacity="0.18" :stroke="fill(index)" stroke-opacity="0.55" class="treemap-cell" @click="drill(rect.node)" />
        <g v-if="labelVisible(rect)" class="treemap-label" @click="drill(rect.node)">
          <text :x="rect.x + 12" :y="rect.y + 23" :fill="fill(index)">{{ rect.node.name }}</text>
          <text :x="rect.x + 12" :y="rect.y + 42" fill="var(--text)">{{ formatBytes(rect.node.sizeBytes) }}</text>
          <text :x="rect.x + 12" :y="rect.y + 58" fill="var(--muted)">{{ formatPercent(rect.pct) }}</text>
        </g>
        <title>{{ rect.node.path }} · {{ formatBytes(rect.node.sizeBytes) }}</title>
      </g>
    </svg>
    <div v-if="!rects.length && !loading" class="treemap-empty">选择卷并开始分析，查看目录占用</div>
    <div v-if="loading" class="treemap-loading"><span class="spinner"></span>正在统计目录…</div>
  </div>
</template>
