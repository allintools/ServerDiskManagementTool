<script setup lang="ts">
import Icon from './Icon.vue'
defineProps<{ title: string; description?: string; icon?: string }>()
</script>

<template>
  <div class="empty-state">
    <span class="empty-icon"><Icon :name="icon ?? 'disk'" :size="24" /></span>
    <h3>{{ title }}</h3>
    <p v-if="description">{{ description }}</p>
    <slot />
  </div>
</template>
