<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { Server, Task } from '../types'
import { api } from '../services/api'
import { notify, reportError } from '../services/ui'
import { openServer } from '../router'
import { useConnectFlow } from '../services/connectFlow'
import Icon from '../components/Icon.vue'
import ServerTable from '../components/ServerTable.vue'
import AddServerDialog from '../components/AddServerDialog.vue'
import ImportConfigDialog from '../components/ImportConfigDialog.vue'
import HostKeyDialog from '../components/HostKeyDialog.vue'

const servers = ref<Server[]>([])
const selected = ref<string[]>([])
const search = ref('')
const group = ref('')
const status = ref('')
const loading = ref(true)
const addOpen = ref(false)
const importOpen = ref(false)
const activeTaskId = ref('')
const deletingId = ref('')

const flow = useConnectFlow(task => { void poll(task) })

const groups = computed(() => [...new Set(servers.value.map(s => s.groupName || '未分组'))].sort())
const filtered = computed(() => servers.value.filter(server => {
  const text = `${server.name} ${server.host} ${server.alias ?? ''} ${server.tags.join(' ')}`.toLowerCase()
  return (!search.value || text.includes(search.value.toLowerCase()))
    && (!group.value || (server.groupName || '未分组') === group.value)
    && (!status.value || server.lastStatus === status.value)
}))

async function load() {
  loading.value = true
  try { servers.value = await api.listServers() } catch (error) { reportError(error) } finally { loading.value = false }
}

async function poll(task: Task) {
  activeTaskId.value = task.id
  try {
    for (let i = 0; i < 600; i++) {
      const detail = await api.getTask(task.id)
      if (!['running', 'queued'].includes(detail.task.status)) {
        await load()
        notify(`${detail.task.title}：${detail.task.status === 'succeeded' ? '完成' : '部分或全部失败'}`, detail.task.status === 'succeeded' ? 'ok' : 'error')
        return
      }
      await new Promise(resolve => setTimeout(resolve, 700))
    }
  } catch (error) { reportError(error) } finally { activeTaskId.value = '' }
}

async function refresh(ids: string[]) {
  if (!ids.length) { notify('请先选择服务器', 'error'); return }
  try {
    const task = await api.startBatchProbe(ids)
    notify(`已开始巡检 ${ids.length} 台服务器`)
    void poll(task)
  } catch (error) { reportError(error) }
}

function toggle(id: string) {
  selected.value = selected.value.includes(id) ? selected.value.filter(x => x !== id) : [...selected.value, id]
}
function toggleAll() {
  const ids = filtered.value.map(s => s.id)
  const all = ids.every(id => selected.value.includes(id))
  selected.value = all ? selected.value.filter(id => !ids.includes(id)) : [...new Set([...selected.value, ...ids])]
}
function handleCreated(server: Server) { servers.value.push(server); flow.enqueue([server]) }
function handleImported(imported: Server[]) { servers.value.push(...imported); flow.enqueue(imported) }

async function removeServer(server: Server) {
  if (deletingId.value) return
  if (!window.confirm(`删除服务器 ${server.name}？\n只删除本机记录和凭据，不会修改远端服务器。`)) return
  deletingId.value = server.id
  try {
    await api.deleteServer(server.id)
    servers.value = servers.value.filter(item => item.id !== server.id)
    selected.value = selected.value.filter(id => id !== server.id)
    notify(`已删除 ${server.name}`)
  } catch (error) {
    reportError(error)
  } finally {
    deletingId.value = ''
  }
}

onMounted(load)
</script>

<template>
  <div class="page servers-page">
    <div class="action-row">
      <div class="action-buttons">
        <button class="button ghost" @click="importOpen = true"><Icon name="download" :size="16" />从 SSH 导入</button>
        <button class="button ghost" :disabled="!selected.length || !!activeTaskId" @click="refresh(selected)"><Icon name="refresh" :size="16" />刷新选中</button>
        <button class="button primary" @click="addOpen = true"><Icon name="plus" :size="16" />添加服务器</button>
      </div>
      <div class="toolbar-filters">
        <label class="search-field"><Icon name="search" :size="15" /><input v-model="search" placeholder="搜索名称、主机、标签…" /></label>
        <select v-model="group" aria-label="按分组筛选"><option value="">全部分组</option><option v-for="g in groups" :key="g" :value="g">{{ g }}</option></select>
        <select v-model="status" aria-label="按状态筛选"><option value="">全部状态</option><option value="online">在线</option><option value="offline">离线</option><option value="degraded">部分异常</option><option value="host_key_unknown">待确认指纹</option><option value="error">错误</option></select>
        <span class="filter-count">{{ filtered.length }} / {{ servers.length }}</span>
      </div>
    </div>

    <section class="panel table-panel">
      <div v-if="selected.length" class="selection-bar">
        <span>已选 {{ selected.length }} 台</span>
        <button class="text-button" :disabled="!!activeTaskId" @click="refresh(selected)">刷新选中</button>
        <button class="text-button" @click="selected = []">清除选择</button>
      </div>
      <ServerTable :servers="filtered" :selected="selected" :deleting-id="deletingId" :empty-text="loading ? '正在加载…' : '没有匹配的服务器'" @toggle="toggle" @toggle-all="toggleAll" @open="id => openServer(id)" @refresh="id => { const s = servers.find(x => x.id === id); if (s) flow.enqueue([s]) }" @remove="removeServer" />
    </section>

    <div class="info-grid">
      <article class="info-card"><Icon name="shield" :size="18" /><div><strong>主机密钥固定</strong><p>首次连接展示指纹，变更时阻断。应用使用独立 known_hosts，不静默接受新密钥。</p></div></article>
      <article class="info-card"><Icon name="key" :size="18" /><div><strong>凭据不落库</strong><p>密码和私钥口令只写入系统凭据存储（macOS 钥匙串 / Windows 凭据管理器）；SQLite 仅保存引用。不支持 sudo 密码。</p></div></article>
      <article class="info-card"><Icon name="terminal" :size="18" /><div><strong>复用系统 SSH</strong><p>通过本机 OpenSSH 连接，保留 ProxyJump、IdentityFile 等配置；不提供任意远程终端。</p></div></article>
    </div>

    <AddServerDialog :open="addOpen" @close="addOpen = false" @created="handleCreated" />
    <ImportConfigDialog :open="importOpen" @close="importOpen = false" @imported="handleImported" />
    <HostKeyDialog :open="flow.show.value" :challenge="flow.challenge.value" :changed="flow.changed.value" @close="flow.closeChallenge" @trusted="flow.trusted" />
  </div>
</template>
