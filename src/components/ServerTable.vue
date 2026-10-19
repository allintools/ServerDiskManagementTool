<script setup lang="ts">
import type { Server } from '../types'
import { timeAgo, formatPercent, osLabel } from '../services/format'
import StatusPill from './StatusPill.vue'
import CapacityBar from './CapacityBar.vue'
import Icon from './Icon.vue'

const props = withDefaults(defineProps<{
  servers: Server[]
  selected?: string[]
  selectable?: boolean
  emptyText?: string
  deletingId?: string | null
}>(), {
  selected: () => [], selectable: true, emptyText: '还没有服务器', deletingId: null,
})
const emit = defineEmits<{
  (e: 'toggle', id: string): void
  (e: 'toggleAll'): void
  (e: 'open', id: string): void
  (e: 'refresh', id: string): void
  (e: 'remove', server: Server): void
}>()

function isSelected(id: string) { return props.selected.includes(id) }
function allSelected() { return props.servers.length > 0 && props.servers.every(s => isSelected(s.id)) }
</script>

<template>
  <div class="table-wrap server-table-wrap">
    <table class="data-table server-table">
      <thead>
        <tr>
          <th v-if="selectable" class="check-cell"><input type="checkbox" :checked="allSelected()" aria-label="全选服务器" @change="emit('toggleAll')" /></th>
          <th>状态</th>
          <th>服务器</th>
          <th>分组</th>
          <th>系统</th>
          <th class="num">最高容量</th>
          <th class="num">卷</th>
          <th class="num">告警</th>
          <th>最后巡检</th>
          <th class="action-cell">操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="server in servers" :key="server.id" class="clickable" @click="emit('open', server.id)">
          <td v-if="selectable" class="check-cell" @click.stop><input type="checkbox" :checked="isSelected(server.id)" :aria-label="`选择 ${server.name}`" @change="emit('toggle', server.id)" /></td>
          <td><StatusPill :status="server.lastStatus" /></td>
          <td>
            <div class="server-name-cell">
              <span class="server-avatar"><Icon name="server" :size="16" /></span>
              <div><strong>{{ server.name }}</strong><small>{{ server.user }}@{{ server.alias || server.host }}:{{ server.port }}</small></div>
            </div>
          </td>
          <td><span class="tag">{{ server.groupName || '未分组' }}</span></td>
          <td class="muted">{{ osLabel(server.summary.os, server.summary.distro) }}</td>
          <td class="num capacity-cell">
            <CapacityBar :pct="server.summary.maxUsedPct" compact />
            <span class="capacity-value">{{ formatPercent(server.summary.maxUsedPct) }}</span>
          </td>
          <td class="num">{{ server.summary.volumeCount }}</td>
          <td class="num">
            <span v-if="server.summary.criticalCount" class="alert-count danger">{{ server.summary.criticalCount }}</span>
            <span v-else-if="server.summary.warningCount" class="alert-count warn">{{ server.summary.warningCount }}</span>
            <span v-else class="muted">—</span>
          </td>
          <td class="muted">{{ timeAgo(server.summary.collectedAt) }}</td>
          <td class="action-cell">
            <button class="icon-button" title="刷新巡检" aria-label="刷新巡检" @click.stop="emit('refresh', server.id)"><Icon name="refresh" :size="15" /></button>
            <button class="icon-button" title="查看详情" aria-label="查看详情" @click.stop="emit('open', server.id)"><Icon name="right" :size="15" /></button>
            <button class="icon-button danger-text" :title="`删除服务器 ${server.name}`" :aria-label="`删除服务器 ${server.name}`" :disabled="deletingId === server.id" @click.stop="emit('remove', server)"><Icon name="trash" :size="15" /></button>
          </td>
        </tr>
        <tr v-if="!servers.length"><td :colspan="selectable ? 10 : 9"><div class="table-empty">{{ emptyText }}</div></td></tr>
      </tbody>
    </table>
  </div>
</template>
