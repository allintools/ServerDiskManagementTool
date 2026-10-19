<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { Task, TaskDetail } from '../types'
import { api } from '../services/api'
import { formatBytes, formatDuration, formatTime, statusText, timeAgo } from '../services/format'
import { notify, reportError } from '../services/ui'
import { openServer } from '../router'
import Icon from '../components/Icon.vue'
import StatusPill from '../components/StatusPill.vue'
import EmptyState from '../components/EmptyState.vue'

const tasks = ref<Task[]>([])
const selectedId = ref('')
const detail = ref<TaskDetail | null>(null)
const loading = ref(true)
const canceling = ref(false)
let timer: ReturnType<typeof setInterval> | undefined

const hasRunning = computed(() => tasks.value.some(t => ['running', 'queued'].includes(t.status)))

async function load() {
  try {
    tasks.value = await api.listTasks(100)
    if (!selectedId.value && tasks.value.length) selectedId.value = tasks.value[0].id
    if (selectedId.value) await loadDetail(selectedId.value)
  } catch (error) { reportError(error) } finally { loading.value = false }
}

async function loadDetail(id: string) {
  try { detail.value = await api.getTask(id) } catch (error) { reportError(error) }
}

async function select(task: Task) {
  selectedId.value = task.id
  await loadDetail(task.id)
}

async function cancel() {
  if (!detail.value || canceling.value) return
  canceling.value = true
  try {
    await api.cancelTask(detail.value.task.id)
    notify('已发送取消请求')
    await load()
  } catch (error) { reportError(error) } finally { canceling.value = false }
}

function kindIcon(kind: string) {
  return kind === 'cleanup' ? 'trash' : kind === 'probe' ? 'scan' : 'activity'
}

onMounted(async () => {
  await load()
  timer = setInterval(async () => {
    if (hasRunning.value) await load()
  }, 1500)
})
onUnmounted(() => clearInterval(timer))
</script>

<template>
  <div class="page tasks-page">
    <div class="action-row">
      <div class="action-buttons"><button class="button ghost" @click="load"><Icon name="refresh" :size="16" />刷新</button></div>
    </div>

    <div class="tasks-layout">
      <section class="panel task-list-panel">
        <header class="panel-head"><div><span class="section-kicker">RUNS</span><h3>任务历史</h3></div><span class="muted">{{ tasks.length }}</span></header>
        <div v-if="loading" class="inline-loading"><Icon name="loader" :size="17" class="spin" />加载中…</div>
        <div v-else-if="!tasks.length" class="empty-inline">还没有任务。巡检或清理后会出现在这里。</div>
        <div v-else class="task-list">
          <button v-for="task in tasks" :key="task.id" class="task-row" :class="{ active: selectedId === task.id }" @click="select(task)">
            <span class="task-icon"><Icon :name="kindIcon(task.kind)" :size="16" /></span>
            <span class="task-row-main"><strong>{{ task.title }}</strong><small>{{ timeAgo(task.startedAt) }} · {{ task.totalHosts }} 台 · {{ task.concurrency }} 并发</small></span>
            <StatusPill :status="task.status" />
          </button>
        </div>
      </section>

      <section class="panel task-detail-panel">
        <template v-if="detail">
          <header class="detail-head">
            <div><span class="section-kicker">{{ detail.task.kind.toUpperCase() }} · {{ detail.task.id.slice(0, 8) }}</span><h2>{{ detail.task.title }}</h2><p>{{ formatTime(detail.task.startedAt) }}<span v-if="detail.task.finishedAt"> → {{ formatTime(detail.task.finishedAt) }}</span></p></div>
            <div class="detail-actions">
              <button v-if="['running', 'queued'].includes(detail.task.status)" class="button danger ghost" :disabled="canceling" @click="cancel"><Icon name="stop" :size="15" />取消任务</button>
              <StatusPill :status="detail.task.status" />
            </div>
          </header>
          <div class="task-stats">
            <div><span>主机</span><strong>{{ detail.task.totalHosts }}</strong></div>
            <div><span>成功</span><strong class="text-ok">{{ detail.task.succeeded }}</strong></div>
            <div><span>部分</span><strong class="text-warn">{{ detail.task.partial }}</strong></div>
            <div><span>失败</span><strong class="text-danger">{{ detail.task.failed }}</strong></div>
            <div><span>取消</span><strong>{{ detail.task.cancelled }}</strong></div>
          </div>
          <div class="host-results">
            <div v-for="host in detail.hosts" :key="host.id" class="host-result">
              <div class="host-result-head">
                <button class="text-button" @click="openServer(host.serverId)"><Icon name="server" :size="14" />{{ host.serverName }}</button>
                <StatusPill :status="host.status" />
                <span class="muted">{{ formatDuration(host.durationMs) }}</span>
                <span v-if="host.bytes" class="released">{{ formatBytes(host.bytes) }} 已释放</span>
              </div>
              <p v-if="host.error" class="host-error">{{ host.error }}</p>
              <details v-if="host.commandPreview || host.outputExcerpt">
                <summary>查看命令与输出</summary>
                <code v-if="host.commandPreview" class="command-preview">{{ host.commandPreview }}</code>
                <pre v-if="host.outputExcerpt" class="output-preview">{{ host.outputExcerpt }}</pre>
              </details>
            </div>
          </div>
        </template>
        <EmptyState v-else title="选择一个任务" description="左侧任务列表会显示逐主机执行结果。" icon="tasks" />
      </section>
    </div>
  </div>
</template>
