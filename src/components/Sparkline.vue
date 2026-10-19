<script setup lang="ts">
import { computed } from 'vue'
const props = withDefaults(defineProps<{ values: number[]; height?: number; color?: string; max?: number }>(), { height: 44, color: 'var(--accent)', max: 0 })
const width = 240
const points = computed(() => {
  const values = props.values.length ? props.values : [0]
  const max = props.max || Math.max(1, ...values)
  const step = width / Math.max(1, values.length - 1)
  return values.map((v, i) => `${(i * step).toFixed(1)},${(props.height - Math.min(1, v / max) * (props.height - 4) - 2).toFixed(1)}`).join(' ')
})
const area = computed(() => `0,${props.height} ${points.value} ${width},${props.height}`)
</script>

<template>
  <svg class="sparkline" :viewBox="`0 0 ${width} ${height}`" preserveAspectRatio="none" aria-hidden="true">
    <polygon :points="area" :fill="color" fill-opacity="0.12" />
    <polyline :points="points" fill="none" :stroke="color" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
  </svg>
</template>
