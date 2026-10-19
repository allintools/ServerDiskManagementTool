<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { Alert, CleanupPlan, Server, Task } from '../types'
import { api } from '../services/api'
import { appSettings, appState } from '../services/store'
import { formatPercent, statusText, timeAgo } from '../services/format'
import { notify, reportError } from '../services/ui'
import { openServer } from '../router'
import { useConnectFlow } from '../services/connectFlow'
import Icon from '../components/Icon.vue'
import ServerTable from '../components/ServerTable.vue'
import CapacityBar from '../components/CapacityBar.vue'
import StatusPill from '../components/StatusPill.vue'
import AddServerDialog from '../components/AddServerDialog.vue'
import ImportConfigDialog from '../components/ImportConfigDialog.vue'
import HostKeyDialog from '../components/HostKeyDialog.vue'
import CleanupPlanDialog from '../components/CleanupPlanDialog.vue'

const servers = ref<Server[]>([])
const alerts = ref<Alert[]>([])
const tasks = ref<Task[]>([])
const selected = ref<string[]>([])
const loading = ref(true)
const deletingId = ref('')
const addOpen = ref(false)
const importOpen = ref(false)
const activeTaskId = ref('')
const plan = ref<CleanupPlan | null>(null)
const planOpen = ref(false)
const preparingPlan = ref(false)
const polling = new Set<string>()
let autoTimer: ReturnType<typeof setInterval> | undefined

const flow = useConnectFlow(task => { upsertTask(task); void pollTask(task.id) })

const online = computed(() => servers.value.filter(s => s.lastStatus === 'online').length)
const atRisk = computed(() => servers.value.filter(s => (s.summary.warningCount + s.summary.criticalCount) > 0).length)
const offline = computed(() => servers.value.filter(s => s.lastStatus === 'offline').length)
const lastCollected = computed(() => servers.value.map(s => s.summary.collectedAt ?? 0).reduce((a, b) => Math.max(a, b), 0))
const topServers = computed(() => [...servers.value].sort((a, b) => (b.summary.maxUsedPct ?? 0) - (a.summary.maxUsedPct ?? 0)).slice(0, 6))
const recentTasks = computed(() => tasks.value.slice(0, 5))

async function load() {
  loading.value = true
  try {
    const [serverList, alertList, taskList] = await Promise.all([
      api.listServers(), api.listAlerts(true, 50), api.listTasks(20),
    ])
    servers.value = serverList
    alerts.value = alertList
    tasks.value = taskList
    appState.lastRefresh = Date.now()
  } catch (error) {
    reportError(error)
  } finally {
    loading.value = false
  }
}

function upsertTask(task: Task) {
  const index = tasks.value.findIndex(t => t.id === task.id)
  if (index >= 0) tasks.value.splice(index, 1, task)
  else tasks.value.unshift(task)
}

async function pollTask(taskId: string) {
  if (polling.has(taskId)) return
  polling.add(taskId)
  activeTaskId.value = taskId
  try {
    for (let i = 0; i < 600; i++) {
      const detail = await api.getTask(taskId)
      upsertTask(detail.task)
      if (!['running', 'queued'].includes(detail.task.status)) {
        await load()
        const failed = detail.hosts.filter(h => ['failed', 'partial'].includes(h.status))
        if (failed.length) notify(`${detail.task.title}：${statusText(detail.task.status)}，${failed.length} 台需关注`, 'error')
        else notify(`${detail.task.title}：${statusText(detail.task.status)}`)
        return
      }
      await new Promise(resolve => setTimeout(resolve, 700))
    }
  } catch (error) {
    reportError(error)
  } finally {
    polling.delete(taskId)
    if (activeTaskId.value === taskId) activeTaskId.value = ''
  }
}

async function refresh(ids: string[], silent = false) {
  if (!ids.length) { notify('请先选择服务器', 'error'); return }
  try {
    const task = await api.startBatchProbe(ids)
    upsertTask(task)
    if (!silent) notify(`已开始巡检 ${ids.length} 台服务器`)
    void pollTask(task.id)
  } catch (error) {
    reportError(error)
  }
}

function refreshAll() { void refresh(servers.value.map(s => s.id)) }
function refreshSelected() { void refresh(selected.value) }
function refreshOne(id: string) {
  const server = servers.value.find(s => s.id === id)
  if (server) flow.enqueue([server])
}

function toggle(id: string) {
  selected.value = selected.value.includes(id) ? selected.value.filter(x => x !== id) : [...selected.value, id]
}
function toggleAll() {
  selected.value = selected.value.length === servers.value.length ? [] : servers.value.map(s => s.id)
}

async function prepareBatchCleanup() {
  if (!selected.value.length || preparingPlan.value) return
  preparingPlan.value = true
  try {
    plan.value = await api.prepareCleanupPlan({
      serverIds: [...selected.value],
      templateIds: ['apt_cache', 'dnf_cache', 'yum_cache', 'pacman_cache', 'apk_cache', 'journal_7d'],
      files: [],
    })
    planOpen.value = true
  } catch (error) {
    reportError(error)
  } finally {
    preparingPlan.value = false
  }
}

async function afterBatchExecuted(taskId: string) {
  planOpen.value = false
  try {
    for (let i = 0; i < 600; i++) {
      const detail = await api.getTask(taskId)
      upsertTask(detail.task)
      if (!['running', 'queued'].includes(detail.task.status)) {
        await load()
        notify(`批量回收：${statusText(detail.task.status)}`, detail.task.status === 'succeeded' ? 'ok' : 'error')
        return
      }
      await new Promise(resolve => setTimeout(resolve, 700))
    }
  } catch (error) {
    reportError(error)
  }
}

function goTasks() { location.hash = '#/tasks' }
function goAlerts() { location.hash = '#/alerts' }

function handleCreated(server: Server) {
  servers.value.push(server)
  notify(`已添加 ${server.name}，正在测试连接`)
  flow.enqueue([server])
}
function handleImported(imported: Server[]) {
  servers.value.push(...imported)
  notify(`已导入 ${imported.length} 个 SSH Host`)
  flow.enqueue(imported)
}

async function removeServer(server: Server) {
  if (deletingId.value) return
  if (!window.confirm(`删除服务器 ${server.name}？\n只删除本机记录和凭据，不会修改远端服务器。`)) return
  deletingId.value = server.id
  try {
    await api.deleteServer(server.id)
    selected.value = selected.value.filter(id => id !== server.id)
    notify(`已删除 ${server.name}`)
    await load()
  } catch (error) {
    reportError(error)
  } finally {
    deletingId.value = ''
  }
}

onMounted(async () => {
  await load()
  autoTimer = setInterval(() => {
    if (document.visibilityState !== 'visible' || !appSettings.autoRefreshSecs || activeTaskId.value) return
    if (!servers.value.length) return
    void refresh(servers.value.map(s => s.id), true)
  }, Math.max(30, appSettings.autoRefreshSecs) * 1000)
})
onUnmounted(() => clearInterval(autoTimer))
</script>

<template>
  <div class="page fleet-page">
    <div class="action-row">
      <div class="action-buttons">
        <button class="button ghost" @click="importOpen = true"><Icon name="download" :size="16" />从 SSH 导入</button>
        <button class="button ghost" :disabled="!!activeTaskId" @click="refreshSelected"><Icon name="refresh" :size="16" />刷新选中<span v-if="selected.length">({{ selected.length }})</span></button>
        <button class="button primary" :disabled="!!activeTaskId" @click="refreshAll"><Icon name="scan" :size="16" />全量巡检</button>
        <button class="button accent" @click="addOpen = true"><Icon name="plus" :size="16" />添加服务器</button>
      </div>
    </div>

    <div class="kpi-grid">
      <article class="kpi-card"><span class="kpi-icon teal"><Icon name="servers" :size="19" /></span><div><small>在线服务器</small><strong>{{ online }}<span> / {{ servers.length }}</span></strong><p>{{ offline ? `${offline} 台离线` : '全部连接正常' }}</p></div></article>
      <article class="kpi-card"><span class="kpi-icon amber"><Icon name="disk" :size="19" /></span><div><small>风险主机</small><strong>{{ atRisk }}</strong><p>容量或 inode 超过阈值</p></div></article>
      <article class="kpi-card"><span class="kpi-icon danger"><Icon name="alerts" :size="19" /></span><div><small>活动告警</small><strong>{{ alerts.length }}</strong><p>{{ alerts.filter(a => a.severity === 'critical').length }} 条严重</p></div></article>
      <article class="kpi-card"><span class="kpi-icon blue"><Icon name="clock" :size="19" /></span><div><small>最近采集</small><strong class="kpi-time">{{ lastCollected ? timeAgo(lastCollected) : '尚未巡检' }}</strong><p>{{ activeTaskId ? '巡检进行中…' : '数据来自真实 SSH' }}</p></div></article>
    </div>

    <div class="dashboard-grid">
      <section class="panel">
        <header class="panel-head"><div><span class="section-kicker">CAPACITY WATCH</span><h3>容量风险</h3></div><button class="text-button" @click="openServer(topServers[0]?.id || '')" :disabled="!topServers.length">查看详情<Icon name="right" :size="13" /></button></header>
        <div v-if="topServers.length" class="risk-list">
          <button v-for="server in topServers" :key="server.id" class="risk-row" @click="openServer(server.id, 'disks')">
            <span class="risk-name"><strong>{{ server.name }}</strong><small>{{ server.summary.distro || server.host }}</small></span>
            <span class="risk-bar"><CapacityBar :pct="server.summary.maxUsedPct" compact /><small>最高卷 {{ formatPercent(server.summary.maxUsedPct) }} · {{ server.summary.volumeCount }} 个卷</small></span>
            <span class="risk-side"><StatusPill :status="server.lastStatus" /><small>{{ timeAgo(server.summary.collectedAt) }}</small></span>
          </button>
        </div>
        <div v-else class="empty-inline">巡检后显示容量风险</div>
      </section>

      <section class="panel">
        <header class="panel-head"><div><span class="section-kicker">RECENT RUNS</span><h3>最近任务</h3></div><button class="text-button" @click="goTasks">全部任务<Icon name="right" :size="13" /></button></header>
        <div v-if="recentTasks.length" class="mini-task-list">
          <button v-for="task in recentTasks" :key="task.id" class="mini-task" @click="goTasks">
            <span class="task-icon"><Icon :name="task.kind === 'cleanup' ? 'trash' : 'scan'" :size="15" /></span>
            <span class="mini-task-main"><strong>{{ task.title }}</strong><small>{{ task.totalHosts }} 台 · {{ timeAgo(task.startedAt) }}</small></span>
            <StatusPill :status="task.status" />
          </button>
        </div>
        <div v-else class="empty-inline">还没有任务</div>
      </section>
    </div>

    <section class="panel table-panel">
      <header class="panel-head"><div><span class="section-kicker">SERVER INVENTORY</span><h3>服务器</h3></div><span class="muted">{{ servers.length }} 台 · 最后刷新 {{ timeAgo(appState.lastRefresh) }}</span></header>
      <div v-if="selected.length" class="selection-bar"><span>已选 {{ selected.length }} 台</span><button class="text-button" @click="refreshSelected">刷新选中</button><button class="text-button" :disabled="preparingPlan" @click="prepareBatchCleanup">{{ preparingPlan ? '生成计划中…' : '批量回收缓存' }}</button><button class="text-button" @click="selected = []">清除</button></div>
      <ServerTable :servers="servers" :selected="selected" :deleting-id="deletingId" :empty-text="loading ? '正在加载…' : '还没有服务器，先添加一台或从 SSH 配置导入'" @toggle="toggle" @toggle-all="toggleAll" @open="id => openServer(id)" @refresh="refreshOne" @remove="removeServer" />
    </section>

    <div v-if="alerts.length" class="alert-strip">
      <div class="alert-strip-head"><span class="section-kicker">ACTIVE ALERTS</span><button class="text-button" @click="goAlerts">查看全部<Icon name="right" :size="13" /></button></div>
      <div class="alert-strip-list">
        <button v-for="alert in alerts.slice(0, 4)" :key="alert.id" class="alert-chip" :class="`severity-${alert.severity}`" @click="openServer(alert.serverId, 'activity')">
          <i></i><strong>{{ alert.serverName }}</strong><span>{{ alert.title }}</span>
        </button>
      </div>
    </div>

    <AddServerDialog :open="addOpen" @close="addOpen = false" @created="handleCreated" />
    <ImportConfigDialog :open="importOpen" @close="importOpen = false" @imported="handleImported" />
    <HostKeyDialog :open="flow.show.value" :challenge="flow.challenge.value" :changed="flow.changed.value" @close="flow.closeChallenge" @trusted="flow.trusted" />
    <CleanupPlanDialog :open="planOpen" :plan="plan" @close="planOpen = false" @executed="afterBatchExecuted" />
  </div>
</template>
