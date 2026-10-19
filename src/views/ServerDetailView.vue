<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type {
  Alert, CleanupPlan, FileCleanupRequest, HostKeyChallenge, LargeFile, MetricSample, QuarantineEntry,
  ReclaimCandidate, Server, Snapshot, Task, UsageNode, UsageScan, VolumeInfo,
} from '../types'
import { api } from '../services/api'
import { appSettings } from '../services/store'
import {
  basename, formatBytes, formatDuration, formatPercent, formatTime, osLabel, privilegeText,
  riskText, shortPath, statusText, timeAgo,
} from '../services/format'
import { notify, reportError } from '../services/ui'
import { navigate, route, setServerTab, type ServerTab } from '../router'
import Icon from '../components/Icon.vue'
import CapacityBar from '../components/CapacityBar.vue'
import StatusPill from '../components/StatusPill.vue'
import Sparkline from '../components/Sparkline.vue'
import Treemap from '../components/Treemap.vue'
import EmptyState from '../components/EmptyState.vue'
import HostKeyDialog from '../components/HostKeyDialog.vue'
import CleanupPlanDialog from '../components/CleanupPlanDialog.vue'

const props = defineProps<{ serverId: string }>()
const server = ref<Server | null>(null)
const snapshot = ref<Snapshot | null>(null)
const alerts = ref<Alert[]>([])
const tasks = ref<Task[]>([])
const quarantine = ref<QuarantineEntry[]>([])
const loading = ref(true)
const refreshing = ref(false)
const testing = ref(false)
const deleting = ref(false)
const challengeOpen = ref(false)
const challenge = ref<HostKeyChallenge | null>(null)
const challengeChanged = ref(false)
let taskPoller: ReturnType<typeof setInterval> | undefined
let activeTaskId = ref('')
let usageTimer: ReturnType<typeof setInterval> | undefined

const tabs: { id: ServerTab; label: string; icon: string }[] = [
  { id: 'overview', label: '概览', icon: 'fleet' },
  { id: 'disks', label: '磁盘与卷', icon: 'disk' },
  { id: 'usage', label: '空间分析', icon: 'analysis' },
  { id: 'activity', label: '活动与隔离区', icon: 'activity' },
]
const tab = computed(() => route.tab)
const realVolumes = computed(() => (snapshot.value?.volumes ?? []).filter(v => !v.virtualFs))
const serverAlerts = computed(() => alerts.value.filter(a => a.serverId === props.serverId))
const serverTasks = computed(() => tasks.value.slice(0, 12))
const capacity = computed(() => {
  const vols = realVolumes.value
  const total = vols.reduce((s, v) => s + v.totalBytes, 0)
  const used = vols.reduce((s, v) => s + v.usedBytes, 0)
  return { total, used, pct: total ? used / total * 100 : 0 }
})

// Metrics
const metrics = ref<MetricSample | null>(null)
const metricsRunning = ref(false)
const cpuHistory = ref<number[]>([])
const memHistory = ref<number[]>([])
const readHistory = ref<number[]>([])
const writeHistory = ref<number[]>([])
let metricsTimer: ReturnType<typeof setInterval> | undefined

async function tickMetrics() {
  try {
    const sample = await api.sampleMetrics(props.serverId)
    metrics.value = sample
    cpuHistory.value = [...cpuHistory.value.slice(-29), sample.cpuPercent]
    const memPct = sample.memoryTotalBytes ? sample.memoryUsedBytes / sample.memoryTotalBytes * 100 : 0
    memHistory.value = [...memHistory.value.slice(-29), memPct]
    const read = sample.disks.reduce((s, d) => s + d.readBytesPerSec, 0)
    const write = sample.disks.reduce((s, d) => s + d.writeBytesPerSec, 0)
    readHistory.value = [...readHistory.value.slice(-29), read]
    writeHistory.value = [...writeHistory.value.slice(-29), write]
  } catch (error) {
    reportError(error)
    stopMetrics()
  }
}
function startMetrics() {
  if (metricsRunning.value) return
  metricsRunning.value = true
  void tickMetrics()
  metricsTimer = setInterval(tickMetrics, 3200)
}
function stopMetrics() {
  metricsRunning.value = false
  if (metricsTimer) clearInterval(metricsTimer)
  metricsTimer = undefined
}

// Usage
const selectedMount = ref('')
const usageScan = ref<UsageScan | null>(null)
const usageRoot = ref<UsageNode | null>(null)
const pathStack = ref<UsageNode[]>([])
const usageChildren = ref<UsageNode[]>([])
const usageLoading = ref(false)
const usageStarting = ref(false)
const largeFiles = ref<LargeFile[]>([])
const largeLoading = ref(false)
const largeThreshold = ref(1024 ** 3)
const recommendations = ref<ReclaimCandidate[]>([])
const recLoading = ref(false)
const selectedTemplates = ref<string[]>([])
const selectedFiles = ref<number[]>([])
const selectedDirs = ref<number[]>([])
const fileAction = ref<'quarantine' | 'delete'>('quarantine')
const plan = ref<CleanupPlan | null>(null)
const planOpen = ref(false)
const planPreparing = ref(false)
const cleanupTaskId = ref('')
const currentDir = computed(() => pathStack.value[pathStack.value.length - 1] ?? null)

async function load() {
  loading.value = true
  try {
    const [s, snap, alertList, taskList, q] = await Promise.all([
      api.getServer(props.serverId),
      api.getLatestSnapshot(props.serverId),
      api.listAlerts(false, 200),
      api.listTasks(100),
      api.listQuarantine(props.serverId),
    ])
    server.value = s
    snapshot.value = snap
    alerts.value = alertList
    tasks.value = taskList
    quarantine.value = q
    if (snap && !selectedMount.value) {
      const pick = snap.volumes.filter(v => !v.virtualFs).sort((a, b) => b.usedPct - a.usedPct)[0]
      selectedMount.value = pick?.mount ?? snap.volumes[0]?.mount ?? ''
    }
  } catch (error) {
    reportError(error)
  } finally {
    loading.value = false
  }
}

async function refresh() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    const task = await api.startBatchProbe([props.serverId])
    activeTaskId.value = task.id
    notify('已开始巡检')
    for (let i = 0; i < 300; i++) {
      const detail = await api.getTask(task.id)
      if (!['running', 'queued'].includes(detail.task.status)) {
        await load()
        const failed = detail.hosts.filter(h => ['failed', 'partial'].includes(h.status))
        if (failed.length) notify(failed[0].error || '巡检部分失败', 'error')
        else notify('巡检完成')
        return
      }
      await new Promise(resolve => setTimeout(resolve, 700))
    }
  } catch (error) {
    reportError(error)
  } finally {
    refreshing.value = false
    activeTaskId.value = ''
  }
}

async function testConnection() {
  if (testing.value) return
  testing.value = true
  try {
    const result = await api.testConnection(props.serverId)
    if (result.status === 'online') { notify(`连接成功 · ${result.latencyMs ?? '—'} ms`); return }
    if (result.hostKey) {
      challenge.value = result.hostKey
      challengeChanged.value = result.status === 'host_key_changed'
      challengeOpen.value = true
    } else {
      notify(result.message || '连接失败', 'error')
    }
  } catch (error) {
    reportError(error)
  } finally {
    testing.value = false
  }
}

async function trusted() {
  challengeOpen.value = false
  challenge.value = null
  notify('主机密钥已信任，开始巡检')
  await refresh()
}

async function removeServer() {
  if (!server.value || deleting.value) return
  if (!window.confirm(`删除服务器 ${server.value.name}？只删除本机记录，不会修改服务器。`)) return
  deleting.value = true
  try {
    await api.deleteServer(props.serverId)
    notify('已删除服务器记录')
    navigate('/servers')
  } catch (error) { reportError(error) } finally { deleting.value = false }
}

// Usage lifecycle
async function initUsage() {
  if (!snapshot.value) return
  const latest = await api.latestUsageScan(props.serverId)
  if (latest) await loadScan(latest.id)
}

async function startUsage() {
  if (!selectedMount.value || usageStarting.value) return
  usageStarting.value = true
  try {
    const scan = await api.startUsageScan(props.serverId, selectedMount.value)
    usageScan.value = scan
    await pollScan(scan.id)
  } catch (error) { reportError(error) } finally { usageStarting.value = false }
}

async function pollScan(scanId: string) {
  usageLoading.value = true
  try {
    for (let i = 0; i < 600; i++) {
      const scan = await api.getUsageScan(scanId)
      usageScan.value = scan
      if (!['running', 'queued'].includes(scan.status)) {
        if (scan.status === 'failed') notify(scan.error || '目录分析失败', 'error')
        await loadScan(scan.id)
        return
      }
      await new Promise(resolve => setTimeout(resolve, 700))
    }
  } catch (error) { reportError(error) } finally { usageLoading.value = false }
}

async function loadScan(scanId: string) {
  usageLoading.value = true
  try {
    usageScan.value = await api.getUsageScan(scanId)
    usageRoot.value = await api.usageRoot(scanId)
    if (usageRoot.value) {
      pathStack.value = [usageRoot.value]
      usageChildren.value = await api.listUsageChildren(scanId, usageRoot.value.id)
    } else {
      pathStack.value = []
      usageChildren.value = []
    }
    largeFiles.value = await api.listLargeFiles(scanId)
  } catch (error) { reportError(error) } finally { usageLoading.value = false }
}

async function openNode(node: UsageNode) {
  if (usageLoading.value) return
  usageLoading.value = true
  try {
    const children = await api.expandUsageNode(node.id)
    const index = pathStack.value.findIndex(n => n.id === node.id)
    if (index >= 0) pathStack.value = pathStack.value.slice(0, index + 1)
    else pathStack.value = [...pathStack.value, node]
    usageChildren.value = children
  } catch (error) { reportError(error) } finally { usageLoading.value = false }
}

async function goCrumb(index: number) {
  const node = pathStack.value[index]
  if (!node || usageLoading.value) return
  pathStack.value = pathStack.value.slice(0, index + 1)
  try {
    usageLoading.value = true
    usageChildren.value = await api.listUsageChildren(usageScan.value!.id, node.id)
  } catch (error) { reportError(error) } finally { usageLoading.value = false }
}

async function doLargeScan() {
  if (!usageScan.value || largeLoading.value) return
  largeLoading.value = true
  try {
    largeFiles.value = await api.scanLargeFiles(usageScan.value.id, largeThreshold.value)
    notify(`大文件扫描完成，共 ${largeFiles.value.length} 个`)
  } catch (error) { reportError(error) } finally { largeLoading.value = false }
}

async function loadRecommendations() {
  if (recLoading.value) return
  recLoading.value = true
  try {
    recommendations.value = await api.getRecommendations(props.serverId)
    selectedTemplates.value = []
    if (!recommendations.value.length) notify('没有检测到可回收模板')
  } catch (error) { reportError(error) } finally { recLoading.value = false }
}

function toggleTemplate(id: string) {
  selectedTemplates.value = selectedTemplates.value.includes(id)
    ? selectedTemplates.value.filter(x => x !== id)
    : [...selectedTemplates.value, id]
}
function toggleFile(id: number) {
  selectedFiles.value = selectedFiles.value.includes(id)
    ? selectedFiles.value.filter(x => x !== id)
    : [...selectedFiles.value, id]
}
function toggleDir(id: number) {
  selectedDirs.value = selectedDirs.value.includes(id)
    ? selectedDirs.value.filter(x => x !== id)
    : [...selectedDirs.value, id]
}

const selectedTotal = computed(() => {
  let total = recommendations.value.filter(r => selectedTemplates.value.includes(r.templateId)).reduce((s, r) => s + r.sizeBytes, 0)
  total += largeFiles.value.filter(f => selectedFiles.value.includes(f.id)).reduce((s, f) => s + f.sizeBytes, 0)
  total += usageChildren.value.filter(n => selectedDirs.value.includes(n.id)).reduce((s, n) => s + n.sizeBytes, 0)
  return total
})
const selectedCount = computed(() => selectedTemplates.value.length + selectedFiles.value.length + selectedDirs.value.length)

async function preparePlan() {
  if (!selectedCount.value || planPreparing.value) {
    notify('请先选择回收项', 'error')
    return
  }
  if (fileAction.value === 'delete' && !appSettings.allowPermanentDelete) {
    notify('永久删除已在设置中关闭', 'error')
    return
  }
  planPreparing.value = true
  try {
    const files: FileCleanupRequest[] = []
    const scanId = usageScan.value?.id
    if (scanId) {
      for (const id of selectedDirs.value) files.push({ scanId, nodeId: id, action: 'quarantine' })
      for (const id of selectedFiles.value) files.push({ scanId, largeFileId: id, action: fileAction.value })
    }
    const created = await api.prepareCleanupPlan({
      serverIds: selectedTemplates.value.length ? [props.serverId] : [],
      templateIds: selectedTemplates.value,
      files,
    })
    plan.value = created
    planOpen.value = true
  } catch (error) {
    reportError(error)
  } finally {
    planPreparing.value = false
  }
}

async function afterExecuted(taskId: string) {
  cleanupTaskId.value = taskId
  planOpen.value = false
  notify('清理任务已启动，执行结果会写入任务与审计')
  try {
    for (let i = 0; i < 300; i++) {
      const detail = await api.getTask(taskId)
      if (!['running', 'queued'].includes(detail.task.status)) {
        await load()
        await loadQuarantine()
        notify(`清理任务：${statusText(detail.task.status)}`, detail.task.status === 'succeeded' ? 'ok' : 'error')
        return
      }
      await new Promise(resolve => setTimeout(resolve, 800))
    }
  } catch (error) { reportError(error) }
}

async function loadQuarantine() {
  try { quarantine.value = await api.listQuarantine(props.serverId) } catch (error) { reportError(error) }
}
async function restoreEntry(entry: QuarantineEntry) {
  if (!window.confirm(`恢复到原路径？\n${entry.originalPath}`)) return
  try { const res = await api.restoreQuarantine(entry.id); notify(res.message); await loadQuarantine() } catch (error) { reportError(error) }
}
async function purgeEntry(entry: QuarantineEntry) {
  if (!window.confirm(`永久清空隔离项？此操作不可恢复。\n${entry.originalPath}`)) return
  try { const res = await api.purgeQuarantine(entry.id); notify(res.message); await loadQuarantine() } catch (error) { reportError(error) }
}

watch(tab, next => {
  if (next === 'usage' && !usageScan.value) void initUsage()
  if (next !== 'overview') stopMetrics()
})
watch(() => props.serverId, async () => { await load(); if (tab.value === 'usage') await initUsage() })

onMounted(async () => {
  await load()
  if (tab.value === 'usage') await initUsage()
})
onUnmounted(() => {
  stopMetrics()
  if (taskPoller) clearInterval(taskPoller)
  if (usageTimer) clearInterval(usageTimer)
})
</script>

<template>
  <div class="page server-detail-page">
    <div v-if="loading" class="inline-loading"><Icon name="loader" :size="20" class="spin" />正在读取服务器…</div>
    <template v-else-if="server">
      <section class="server-hero panel">
        <div class="hero-main">
          <button class="back-link" @click="navigate('/servers')"><Icon name="right" :size="14" class="flip" />服务器</button>
          <div class="hero-title"><span class="server-avatar large"><Icon name="server" :size="24" /></span><div><div class="hero-name"><h2>{{ server.name }}</h2><StatusPill :status="server.lastStatus" /><span v-if="server.groupName" class="tag">{{ server.groupName }}</span></div><p>{{ server.user }}@{{ server.alias || server.host }}:{{ server.port }}<span class="dot-sep"></span>{{ osLabel(server.summary.os, server.summary.distro) }}<span class="dot-sep"></span>{{ privilegeText(server.summary.privilege) }}</p></div></div>
        </div>
        <div class="hero-actions">
          <span class="muted small">最后巡检 {{ timeAgo(server.summary.collectedAt) }}</span>
          <button class="button ghost" :disabled="testing" @click="testConnection"><Icon name="key" :size="15" />{{ testing ? '测试中…' : '测试连接' }}</button>
          <button class="button primary" :disabled="refreshing" @click="refresh"><Icon name="refresh" :size="15" />{{ refreshing ? '巡检中…' : '刷新巡检' }}</button>
          <button class="icon-button danger-text" title="删除服务器记录" aria-label="删除服务器记录" :disabled="deleting" @click="removeServer"><Icon name="trash" :size="16" /></button>
        </div>
      </section>

      <nav class="tabs" aria-label="服务器详情导航">
        <button v-for="item in tabs" :key="item.id" class="tab" :class="{ active: tab === item.id }" @click="setServerTab(item.id)"><Icon :name="item.icon" :size="15" />{{ item.label }}</button>
      </nav>

      <template v-if="tab === 'overview'">
        <div class="kpi-grid four">
          <article class="kpi-card"><span class="kpi-icon teal"><Icon name="disk" :size="19" /></span><div><small>最高容量</small><strong>{{ formatPercent(server.summary.maxUsedPct) }}</strong><p>{{ server.summary.volumeCount }} 个卷 · {{ server.summary.readOnlyCount }} 个只读</p></div></article>
          <article class="kpi-card"><span class="kpi-icon blue"><Icon name="cpu" :size="19" /></span><div><small>CPU / 负载</small><strong>{{ metrics ? `${metrics.cpuPercent.toFixed(1)}%` : '—' }}</strong><p>{{ metrics ? `load ${metrics.load1.toFixed(2)}` : `${server.summary.cpuCount ?? '—'} 核` }}</p></div></article>
          <article class="kpi-card"><span class="kpi-icon amber"><Icon name="memory" :size="19" /></span><div><small>内存</small><strong>{{ metrics ? formatPercent(metrics.memoryUsedBytes / Math.max(1, metrics.memoryTotalBytes) * 100, 0) : '—' }}</strong><p>{{ metrics ? `${formatBytes(metrics.memoryUsedBytes)} / ${formatBytes(metrics.memoryTotalBytes)}` : formatBytes(server.summary.memoryTotalBytes) }}</p></div></article>
          <article class="kpi-card"><span class="kpi-icon danger"><Icon name="alerts" :size="19" /></span><div><small>活动告警</small><strong>{{ serverAlerts.filter(a => !a.resolvedAt).length }}</strong><p>{{ serverAlerts.filter(a => a.severity === 'critical' && !a.resolvedAt).length }} 条严重</p></div></article>
        </div>

        <div class="detail-grid">
          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">CAPACITY</span><h3>卷容量</h3></div><button class="text-button" @click="setServerTab('disks')">全部卷<Icon name="right" :size="13" /></button></header>
            <div class="capacity-list">
              <div v-for="volume in realVolumes.slice(0, 5)" :key="volume.mount" class="capacity-row">
                <div class="capacity-row-head"><strong>{{ volume.mount }}</strong><span>{{ formatBytes(volume.usedBytes) }} / {{ formatBytes(volume.totalBytes) }}</span></div>
                <CapacityBar :pct="volume.usedPct" :warn="appSettings.thresholdWarn" :critical="appSettings.thresholdCritical" />
                <div class="capacity-meta"><span>{{ volume.fsType }} · {{ volume.device }}</span><span v-if="volume.inodePct != null">inode {{ formatPercent(volume.inodePct) }}</span><span v-if="volume.readOnly" class="text-warn">只读挂载</span></div>
              </div>
              <div v-if="!realVolumes.length" class="empty-inline">还没有巡检数据</div>
            </div>
          </section>

          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">SYSTEM</span><h3>系统信息</h3></div><span class="muted small">{{ formatTime(snapshot?.collectedAt) }}</span></header>
            <dl class="info-list">
              <div><dt>发行版</dt><dd>{{ osLabel(server.summary.os, server.summary.distro) }}</dd></div>
              <div><dt>内核</dt><dd><code>{{ server.summary.kernel || '—' }}</code></dd></div>
              <div><dt>架构</dt><dd>{{ server.summary.arch || '—' }}</dd></div>
              <div><dt>运行时间</dt><dd>{{ server.summary.uptimeSecs ? formatDuration(server.summary.uptimeSecs * 1000) : '—' }}</dd></div>
              <div><dt>权限</dt><dd>{{ privilegeText(server.summary.privilege) }}</dd></div>
              <div><dt>采集耗时</dt><dd>{{ formatDuration(server.summary.latencyMs) }}</dd></div>
              <div><dt>虚拟化</dt><dd>{{ snapshot?.capabilities.containerized ? '容器环境' : '物理机 / 虚拟机' }}</dd></div>
              <div><dt>sudo</dt><dd>{{ snapshot?.capabilities.sudoNopass ? '免密 sudo 可用' : '无免密 sudo' }}</dd></div>
            </dl>
          </section>
        </div>

        <section class="panel metrics-panel">
          <header class="panel-head"><div><span class="section-kicker">LIVE · 3S SAMPLE</span><h3>实时指标</h3></div><button class="button ghost small" @click="metricsRunning ? stopMetrics() : startMetrics()"><Icon :name="metricsRunning ? 'stop' : 'play'" :size="14" />{{ metricsRunning ? '停止采样' : '开始采样' }}</button></header>
          <div class="metrics-grid">
            <div class="metric-block"><div class="metric-label"><span>CPU</span><strong>{{ metrics ? `${metrics.cpuPercent.toFixed(1)}%` : '—' }}</strong></div><Sparkline :values="cpuHistory" color="var(--accent)" :max="100" /></div>
            <div class="metric-block"><div class="metric-label"><span>内存</span><strong>{{ metrics ? formatPercent(metrics.memoryUsedBytes / Math.max(1, metrics.memoryTotalBytes) * 100, 0) : '—' }}</strong></div><Sparkline :values="memHistory" color="var(--blue)" :max="100" /></div>
            <div class="metric-block"><div class="metric-label"><span>磁盘读取</span><strong>{{ formatBytes(metrics?.disks.reduce((s, d) => s + d.readBytesPerSec, 0) ?? 0) }}/s</strong></div><Sparkline :values="readHistory" color="var(--teal)" /></div>
            <div class="metric-block"><div class="metric-label"><span>磁盘写入</span><strong>{{ formatBytes(metrics?.disks.reduce((s, d) => s + d.writeBytesPerSec, 0) ?? 0) }}/s</strong></div><Sparkline :values="writeHistory" color="var(--amber)" /></div>
          </div>
          <p v-if="!metricsRunning" class="form-hint">点击“开始采样”后每 3 秒通过 SSH 读取一次 /proc 指标；离开概览标签会停止。</p>
        </section>

        <section v-if="serverAlerts.filter(a => !a.resolvedAt).length" class="panel">
          <header class="panel-head"><div><span class="section-kicker">ACTIVE ALERTS</span><h3>活动告警</h3></div><button class="text-button" @click="navigate('/alerts')">告警中心<Icon name="right" :size="13" /></button></header>
          <div class="alert-list compact-list">
            <div v-for="alert in serverAlerts.filter(a => !a.resolvedAt).slice(0, 4)" :key="alert.id" class="alert-row" :class="`severity-${alert.severity}`">
              <span class="alert-severity"><Icon name="warning" :size="16" />{{ alert.severity === 'critical' ? '严重' : '警告' }}</span>
              <div class="alert-main"><strong>{{ alert.title }}</strong><p>{{ alert.detail }}</p></div>
              <small class="muted">{{ timeAgo(alert.lastSeenAt) }}</small>
            </div>
          </div>
        </section>
      </template>

      <template v-else-if="tab === 'disks'">
        <section class="panel">
          <header class="panel-head"><div><span class="section-kicker">BLOCK DEVICES</span><h3>物理磁盘</h3></div><span class="muted">{{ snapshot?.disks.length ?? 0 }} 块</span></header>
          <div v-if="snapshot?.disks.length" class="table-wrap">
            <table class="data-table"><thead><tr><th>名称</th><th>型号</th><th>类型</th><th class="num">容量</th><th>旋转盘</th><th>只读</th><th>序列号</th></tr></thead>
              <tbody><tr v-for="disk in snapshot.disks" :key="disk.name"><td><code>{{ disk.name }}</code></td><td>{{ disk.model || '—' }}</td><td><span class="tag">{{ disk.diskType }}</span></td><td class="num">{{ formatBytes(disk.sizeBytes) }}</td><td>{{ disk.rotational ? '是' : '否' }}</td><td>{{ disk.readOnly ? '是' : '否' }}</td><td class="muted">{{ disk.serial || '—' }}</td></tr></tbody>
            </table>
          </div>
          <EmptyState v-else icon="disk" title="没有磁盘信息" description="远端可能缺少 lsblk，或尚未巡检。" />
        </section>

        <section class="panel">
          <header class="panel-head"><div><span class="section-kicker">FILESYSTEMS</span><h3>卷与挂载点</h3></div><span class="muted">按挂载路径排序</span></header>
          <div v-if="snapshot?.volumes.length" class="table-wrap">
            <table class="data-table"><thead><tr><th>挂载</th><th>文件系统</th><th>设备</th><th class="num">容量</th><th class="num">已用</th><th class="num">使用率</th><th class="num">inode</th><th>状态</th><th class="action-cell">分析</th></tr></thead>
              <tbody><tr v-for="volume in snapshot.volumes" :key="volume.mount" :class="{ 'virtual-row': volume.virtualFs }">
                <td><code>{{ volume.mount }}</code><small v-if="volume.virtualFs" class="cell-sub">虚拟文件系统</small></td>
                <td class="muted">{{ volume.fsType || '—' }}</td>
                <td class="muted">{{ volume.device || '—' }}</td>
                <td class="num">{{ formatBytes(volume.totalBytes) }}</td>
                <td class="num">{{ formatBytes(volume.usedBytes) }}</td>
                <td class="num usage-cell"><CapacityBar :pct="volume.usedPct" compact /><span>{{ formatPercent(volume.usedPct) }}</span></td>
                <td class="num">{{ formatPercent(volume.inodePct) }}</td>
                <td><StatusPill v-if="volume.readOnly" status="warning" label="只读" /><span v-else class="muted small">可写</span></td>
                <td class="action-cell"><button v-if="!volume.virtualFs" class="text-button" @click="selectedMount = volume.mount; setServerTab('usage')">分析<Icon name="right" :size="13" /></button></td>
              </tr></tbody>
            </table>
          </div>
          <EmptyState v-else icon="disk" title="没有卷信息" description="请先执行巡检。" />
        </section>
      </template>

      <template v-else-if="tab === 'usage'">
        <section class="panel usage-toolbar">
          <div class="usage-controls">
            <label class="field inline"><span>分析卷</span><select v-model="selectedMount" :disabled="usageStarting || usageLoading"><option v-for="volume in realVolumes" :key="volume.mount" :value="volume.mount">{{ volume.mount }} · {{ formatPercent(volume.usedPct) }}</option></select></label>
            <button class="button primary" :disabled="!selectedMount || usageStarting || usageLoading" @click="startUsage"><Icon name="scan" :size="16" />{{ usageStarting ? '启动中…' : usageScan ? '重新分析根目录' : '开始分析' }}</button>
            <span v-if="usageScan" class="muted small">最近 {{ timeAgo(usageScan.startedAt) }} · {{ formatBytes(usageScan.totalBytes) }} · {{ usageScan.entries }} 项</span>
          </div>
          <div class="usage-legend"><span><i style="background: var(--teal)"></i>目录</span><span>点击色块或表格向下展开一层</span></div>
        </section>

        <div v-if="usageScan?.partial" class="notice"><Icon name="warning" :size="16" /><p>分析结果不完整：部分目录权限不足或读取失败。数据只代表已扫描到的范围。</p></div>

        <section class="panel usage-panel">
          <header class="panel-head">
            <div class="breadcrumbs"><button class="crumb" @click="goCrumb(0)">{{ selectedMount || '/' }}</button><template v-for="(node, index) in pathStack.slice(1)" :key="node.id"><Icon name="right" :size="12" /><button class="crumb" @click="goCrumb(index + 1)">{{ node.name }}</button></template></div>
            <span v-if="currentDir" class="muted small">{{ formatBytes(currentDir.sizeBytes) }} · {{ currentDir.childCount }} 个子项</span>
          </header>
          <div v-if="usageLoading && !usageChildren.length" class="inline-loading"><Icon name="loader" :size="18" class="spin" />正在统计目录…</div>
          <EmptyState v-else-if="!usageScan" icon="analysis" title="还没有目录分析" description="选择一个卷并开始分析；每次只展开一层，避免远程全盘扫描。" />
          <template v-else>
            <Treemap :nodes="usageChildren" :total="currentDir?.sizeBytes || usageScan.totalBytes" :loading="usageLoading" @drill="openNode" />
            <div class="table-wrap usage-table-wrap">
              <table class="data-table"><thead><tr><th class="check-cell"></th><th>目录 / 文件</th><th class="num">大小</th><th class="num">占比</th><th class="num">子项</th><th class="action-cell">操作</th></tr></thead>
                <tbody><tr v-for="node in usageChildren" :key="node.id">
                  <td class="check-cell"><input type="checkbox" :checked="selectedDirs.includes(node.id)" :aria-label="`选择 ${node.name}`" @change="toggleDir(node.id)" @click.stop /></td>
                  <td><button class="text-button path-button" @click="openNode(node)"><Icon name="folder" :size="15" /><strong>{{ node.name }}</strong><small>{{ shortPath(node.path, 70) }}</small></button></td>
                  <td class="num">{{ formatBytes(node.sizeBytes) }}</td>
                  <td class="num">{{ formatPercent(node.sizeBytes / Math.max(1, currentDir?.sizeBytes || 1) * 100) }}</td>
                  <td class="num">{{ node.childCount }}</td>
                  <td class="action-cell"><button class="text-button" @click="openNode(node)">展开<Icon name="right" :size="12" /></button></td>
                </tr></tbody>
              </table>
            </div>
          </template>
        </section>

        <div class="detail-grid">
          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">LARGE FILES</span><h3>大文件</h3></div><div class="inline-controls"><select v-model.number="largeThreshold" aria-label="大文件阈值"><option :value="1024 ** 3">≥ 1 GiB</option><option :value="100 * 1024 ** 2">≥ 100 MiB</option><option :value="10 * 1024 ** 3">≥ 10 GiB</option></select><button class="button ghost small" :disabled="!usageScan || largeLoading" @click="doLargeScan"><Icon name="scan" :size="14" />{{ largeLoading ? '扫描中…' : '扫描' }}</button></div></header>
            <div v-if="largeFiles.length" class="large-file-list">
              <label v-for="file in largeFiles.slice(0, 100)" :key="file.id" class="large-file-row">
                <input type="checkbox" :checked="selectedFiles.includes(file.id)" @change="toggleFile(file.id)" />
                <span class="large-file-main"><strong>{{ basename(file.path) }}</strong><small>{{ shortPath(file.path, 78) }}</small></span>
                <span class="large-file-size">{{ formatBytes(file.sizeBytes) }}</span>
              </label>
            </div>
            <EmptyState v-else icon="file" title="还没有大文件结果" description="分析根目录后点击扫描；仅支持 GNU find 的远端。" />
            <div class="file-action-row"><span class="muted small">已选 {{ selectedFiles.length }} 个</span><select v-model="fileAction" class="compact-select" aria-label="文件处理方式"><option value="quarantine">移动到隔离区（可恢复）</option><option value="delete" :disabled="!appSettings.allowPermanentDelete">永久删除（设置中开启）</option></select></div>
          </section>

          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">RECLAIM TEMPLATES</span><h3>回收候选</h3></div><button class="button ghost small" :disabled="recLoading" @click="loadRecommendations"><Icon name="refresh" :size="14" />{{ recLoading ? '检测中…' : '检测' }}</button></header>
            <div v-if="recommendations.length" class="reclaim-list">
              <label v-for="candidate in recommendations" :key="candidate.templateId" class="reclaim-row">
                <input type="checkbox" :checked="selectedTemplates.includes(candidate.templateId)" @change="toggleTemplate(candidate.templateId)" />
                <span class="reclaim-main"><strong>{{ candidate.title }}<span class="tag" :class="`risk-${candidate.risk}`">{{ riskText(candidate.risk) }}</span></strong><small>{{ candidate.description }}</small><code>{{ candidate.commandPreview }}</code></span>
                <span class="reclaim-size">{{ formatBytes(candidate.sizeBytes) }}</span>
              </label>
            </div>
            <EmptyState v-else icon="package" title="检测可回收缓存" description="只读检测包管理器、journal、Docker 与用户缓存；不会执行。" />
          </section>
        </div>

        <section class="panel plan-action-bar">
          <div><span class="section-kicker">CONTROLLED CLEANUP</span><h3>生成空间回收计划</h3><p>已选 {{ selectedCount }} 项 · 预计 {{ formatBytes(selectedTotal) }}。计划需在有效期内确认，执行前会重新检查目标。</p></div>
          <button class="button danger" :disabled="!selectedCount || planPreparing" @click="preparePlan"><Icon name="trash" :size="16" />{{ planPreparing ? '生成中…' : '生成回收计划' }}</button>
        </section>

        <section class="panel">
          <header class="panel-head"><div><span class="section-kicker">QUARANTINE</span><h3>隔离区</h3></div><span class="muted">{{ quarantine.filter(q => q.status === 'stored').length }} 项待处理</span></header>
          <div v-if="quarantine.length" class="quarantine-list">
            <div v-for="entry in quarantine" :key="entry.id" class="quarantine-row">
              <div><strong>{{ basename(entry.originalPath) }}</strong><small>{{ entry.originalPath }}</small><small class="muted">隔离于 {{ formatTime(entry.createdAt) }} · {{ entry.status === 'stored' ? `保留至 ${formatTime(entry.expiresAt)}` : entry.status }}</small></div>
              <span class="tag">{{ formatBytes(entry.sizeBytes) }}</span>
              <div class="quarantine-actions"><button v-if="entry.status === 'stored'" class="button ghost small" @click="restoreEntry(entry)">恢复</button><button v-if="entry.status === 'stored'" class="button ghost small danger-text" @click="purgeEntry(entry)">清空</button><StatusPill v-if="entry.status !== 'stored'" :status="entry.status" /></div>
            </div>
          </div>
          <EmptyState v-else icon="trash" title="隔离区为空" description="选中大文件或目录并生成隔离计划后，会出现在这里。" />
        </section>
      </template>

      <template v-else-if="tab === 'activity'">
        <div class="detail-grid">
          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">ALERTS</span><h3>该服务器的告警</h3></div></header>
            <div v-if="serverAlerts.length" class="alert-list compact-list">
              <div v-for="alert in serverAlerts" :key="alert.id" class="alert-row" :class="[`severity-${alert.severity}`, { resolved: alert.resolvedAt }]">
                <span class="alert-severity">{{ alert.severity === 'critical' ? '严重' : '警告' }}</span>
                <div class="alert-main"><strong>{{ alert.title }}</strong><p>{{ alert.detail }}</p><small>{{ timeAgo(alert.lastSeenAt) }}<span v-if="alert.resolvedAt"> · 已恢复</span></small></div>
              </div>
            </div>
            <EmptyState v-else icon="shield" title="没有告警" description="巡检后会按阈值自动评估。" />
          </section>
          <section class="panel">
            <header class="panel-head"><div><span class="section-kicker">RECENT TASKS</span><h3>最近任务</h3></div><button class="text-button" @click="navigate('/tasks')">任务中心<Icon name="right" :size="13" /></button></header>
            <div v-if="serverTasks.length" class="mini-task-list">
              <button v-for="task in serverTasks" :key="task.id" class="mini-task" @click="navigate('/tasks')"><span class="task-icon"><Icon name="scan" :size="15" /></span><span class="mini-task-main"><strong>{{ task.title }}</strong><small>{{ timeAgo(task.startedAt) }} · {{ task.totalHosts }} 台</small></span><StatusPill :status="task.status" /></button>
            </div>
            <EmptyState v-else icon="tasks" title="还没有任务" />
          </section>
        </div>
        <section class="panel">
          <header class="panel-head"><div><span class="section-kicker">SERVER FACTS</span><h3>连接与能力</h3></div></header>
          <dl class="info-list two-col">
            <div><dt>SSH 目标</dt><dd><code>{{ server.user }}@{{ server.alias || server.host }}:{{ server.port }}</code></dd></div>
            <div><dt>认证方式</dt><dd>{{ server.authMode === 'agent' ? 'SSH agent' : server.authMode === 'key' ? '私钥' : '密码' }}{{ server.hasSecret ? ' · 已保存凭据' : '' }}</dd></div>
            <div><dt>包管理器</dt><dd>{{ snapshot?.capabilities.packageManagers.join(', ') || '未检测' }}</dd></div>
            <div><dt>du -0</dt><dd>{{ snapshot?.capabilities.duZero ? '支持' : '不支持（使用近似值）' }}</dd></div>
            <div><dt>GNU find</dt><dd>{{ snapshot?.capabilities.gnuFind ? '支持' : '不支持大文件扫描' }}</dd></div>
            <div><dt>Docker</dt><dd>{{ snapshot?.capabilities.docker ? '已检测到' : '未检测到' }}</dd></div>
          </dl>
        </section>
      </template>
    </template>

    <HostKeyDialog :open="challengeOpen" :challenge="challenge" :changed="challengeChanged" @close="challengeOpen = false" @trusted="trusted" />
    <CleanupPlanDialog :open="planOpen" :plan="plan" @close="planOpen = false" @executed="afterExecuted" />
  </div>
</template>
