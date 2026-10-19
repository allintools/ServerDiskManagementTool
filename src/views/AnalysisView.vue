<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { Server, Snapshot, VolumeInfo } from '../types'
import { api } from '../services/api'
import { formatBytes, formatPercent, osLabel, timeAgo } from '../services/format'
import { reportError } from '../services/ui'
import { openServer } from '../router'
import Icon from '../components/Icon.vue'
import CapacityBar from '../components/CapacityBar.vue'
import EmptyState from '../components/EmptyState.vue'
import StatusPill from '../components/StatusPill.vue'

interface Row { server: Server; volume: VolumeInfo }
const servers = ref<Server[]>([])
const snapshots = ref<Record<string, Snapshot | null>>({})
const loading = ref(true)
const sort = ref<'usage' | 'inode' | 'size'>('usage')
const onlyRisk = ref(false)

const rows = computed<Row[]>(() => {
  const list: Row[] = []
  for (const server of servers.value) {
    const snap = snapshots.value[server.id]
    if (!snap) continue
    for (const volume of snap.volumes) {
      if (volume.virtualFs) continue
      if (onlyRisk.value && volume.usedPct < 70 && (volume.inodePct ?? 0) < 70 && !volume.readOnly) continue
      list.push({ server, volume })
    }
  }
  return list.sort((a, b) => {
    if (sort.value === 'inode') return (b.volume.inodePct ?? 0) - (a.volume.inodePct ?? 0)
    if (sort.value === 'size') return b.volume.totalBytes - a.volume.totalBytes
    return b.volume.usedPct - a.volume.usedPct
  })
})

const scanned = computed(() => Object.values(snapshots.value).filter(Boolean).length)
const riskCount = computed(() => rows.value.filter(r => r.volume.usedPct >= 80 || (r.volume.inodePct ?? 0) >= 85 || r.volume.readOnly).length)
const totalUsed = computed(() => rows.value.reduce((sum, r) => sum + r.volume.usedBytes, 0))

async function load() {
  loading.value = true
  try {
    servers.value = await api.listServers()
    const entries = await Promise.all(servers.value.map(async server => {
      try { return [server.id, await api.getLatestSnapshot(server.id)] as const }
      catch { return [server.id, null] as const }
    }))
    snapshots.value = Object.fromEntries(entries)
  } catch (error) { reportError(error) } finally { loading.value = false }
}

onMounted(load)
</script>

<template>
  <div class="page analysis-page">
    <div class="action-row">
      <div class="action-buttons">
        <button class="button ghost" @click="load"><Icon name="refresh" :size="16" />重新加载</button>
      </div>
      <div class="toolbar-filters">
        <label class="check-line compact"><input v-model="onlyRisk" type="checkbox" />仅看风险</label>
        <select v-model="sort" aria-label="排序"><option value="usage">按使用率</option><option value="inode">按 inode</option><option value="size">按容量</option></select>
      </div>
    </div>

    <div class="kpi-grid three">
      <article class="kpi-card"><span class="kpi-icon teal"><Icon name="database" :size="19" /></span><div><small>已巡检主机</small><strong>{{ scanned }}<span> / {{ servers.length }}</span></strong><p>来自最近一次快照</p></div></article>
      <article class="kpi-card"><span class="kpi-icon amber"><Icon name="warning" :size="19" /></span><div><small>风险卷</small><strong>{{ riskCount }}</strong><p>容量、inode 或只读</p></div></article>
      <article class="kpi-card"><span class="kpi-icon blue"><Icon name="chart" :size="19" /></span><div><small>已用空间</small><strong>{{ formatBytes(totalUsed) }}</strong><p>仅统计真实文件系统</p></div></article>
    </div>

    <section class="panel table-panel">
      <header class="panel-head"><div><span class="section-kicker">VOLUME MATRIX</span><h3>卷视图</h3></div></header>
      <div v-if="loading" class="inline-loading"><Icon name="loader" :size="18" class="spin" />正在读取快照…</div>
      <EmptyState v-else-if="!rows.length" icon="analysis" title="还没有可比较的卷" description="先在机群页对服务器执行巡检。" />
      <div v-else class="table-wrap">
        <table class="data-table">
          <thead><tr><th>服务器</th><th>状态</th><th>挂载</th><th>文件系统</th><th class="num">容量</th><th class="num">已用</th><th class="num">使用率</th><th class="num">inode</th><th>最后巡检</th></tr></thead>
          <tbody>
            <tr v-for="row in rows" :key="`${row.server.id}-${row.volume.mount}`" class="clickable" @click="openServer(row.server.id, 'disks')">
              <td><strong>{{ row.server.name }}</strong><small class="cell-sub">{{ osLabel(row.server.summary.os, row.server.summary.distro) }}</small></td>
              <td><StatusPill :status="row.volume.readOnly ? 'warning' : row.server.lastStatus" :label="row.volume.readOnly ? '只读' : undefined" /></td>
              <td><code>{{ row.volume.mount }}</code></td>
              <td class="muted">{{ row.volume.fsType }}</td>
              <td class="num">{{ formatBytes(row.volume.totalBytes) }}</td>
              <td class="num">{{ formatBytes(row.volume.usedBytes) }}</td>
              <td class="num usage-cell"><CapacityBar :pct="row.volume.usedPct" compact /><span>{{ formatPercent(row.volume.usedPct) }}</span></td>
              <td class="num" :class="{ 'text-danger': (row.volume.inodePct ?? 0) >= 90 }">{{ formatPercent(row.volume.inodePct) }}</td>
              <td class="muted">{{ timeAgo(row.server.summary.collectedAt) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <div class="info-grid">
      <article class="info-card"><Icon name="disk" :size="18" /><div><strong>df 与 du 的差异</strong><p>df 包含保留块与文件系统元数据；du 只统计遍历到的文件。两个数字不一致是正常的。</p></div></article>
      <article class="info-card"><Icon name="shield" :size="18" /><div><strong>只读挂载</strong><p>只读卷会在容量列中单独标记。清理前会再次检查挂载状态，只读目标会被拒绝。</p></div></article>
      <article class="info-card"><Icon name="trash" :size="18" /><div><strong>回收入口</strong><p>进入服务器详情 → 空间分析，查看回收候选并生成计划。批量写操作不会自动执行。</p></div></article>
    </div>
  </div>
</template>
