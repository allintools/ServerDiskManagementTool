<script setup lang="ts">
import { computed } from 'vue'
import { statusText, statusTone } from '../services/format'

const props = defineProps<{ status: string; label?: string; dot?: boolean }>()
const tone = computed(() => statusTone(props.status))
</script>

<template>
  <span class="status-pill" :class="`tone-${tone}`">
    <i v-if="dot !== false" class="status-dot"></i>{{ label ?? statusText(status) }}
  </span>
</template>
