<script setup lang="ts">
import { computed } from 'vue'
import { formatPercent } from '../services/format'

const props = defineProps<{ pct: number | null | undefined; warn?: number; critical?: number; compact?: boolean; label?: string }>()
const tone = computed(() => {
  const p = props.pct ?? 0
  if (p >= (props.critical ?? 90)) return 'danger'
  if (p >= (props.warn ?? 80)) return 'warn'
  return 'ok'
})
const width = computed(() => `${Math.min(100, Math.max(0, props.pct ?? 0))}%`)
</script>

<template>
  <div class="capacity" :class="{ compact }">
    <div v-if="label" class="capacity-head"><span>{{ label }}</span><strong :class="`text-${tone}`">{{ formatPercent(pct) }}</strong></div>
    <div class="capacity-track" :class="`tone-${tone}`"><span :style="{ width }"></span></div>
  </div>
</template>
