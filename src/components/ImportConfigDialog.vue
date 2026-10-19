<script setup lang="ts">
import { ref, watch } from 'vue'
import { api } from '../services/api'
import type { ImportCandidate, Server } from '../types'
import { reportError } from '../services/ui'
import Icon from './Icon.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'imported', servers: Server[]): void }>()
const loading = ref(false)
const candidates = ref<ImportCandidate[]>([])
const selected = ref<string[]>([])
const importing = ref(false)

watch(() => props.open, async open => {
  if (!open) return
  loading.value = true
  selected.value = []
  try {
    candidates.value = await api.importSshConfig()
  } catch (error) {
    reportError(error)
  } finally {
    loading.value = false
  }
})

function toggle(alias: string) {
  selected.value = selected.value.includes(alias)
    ? selected.value.filter(a => a !== alias)
    : [...selected.value, alias]
}

async function doImport() {
  if (!selected.value.length || importing.value) return
  importing.value = true
  const imported: Server[] = []
  try {
    for (const alias of selected.value) {
      const candidate = candidates.value.find(c => c.alias === alias)
      if (!candidate) continue
      const server = await api.createServer({
        name: candidate.alias, alias: candidate.alias, host: candidate.host, port: candidate.port,
        user: candidate.user, authMode: 'agent', keyPath: candidate.identityFile || null,
        groupName: 'SSH 配置', tags: ['imported'], secret: null, rememberSecret: false,
      })
      imported.push(server)
    }
    emit('imported', imported)
    emit('close')
  } catch (error) {
    reportError(error)
  } finally {
    importing.value = false
  }
}
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal import-modal" role="dialog" aria-modal="true" aria-labelledby="import-title">
      <header class="modal-head">
        <div><span class="eyebrow">~/.SSH/CONFIG</span><h2 id="import-title">从 SSH 配置导入</h2></div>
        <button class="icon-button" aria-label="关闭" @click="emit('close')"><Icon name="x" :size="18" /></button>
      </header>
      <div class="modal-body">
        <p class="muted">只读取具体 Host 别名；不会修改你的 SSH 配置，也不会复制私钥。导入后使用系统 OpenSSH 解析 ProxyJump、IdentityFile 等选项。</p>
        <div v-if="loading" class="inline-loading"><Icon name="loader" :size="18" class="spin" />正在解析 SSH 配置…</div>
        <div v-else-if="!candidates.length" class="empty-state compact"><Icon name="terminal" :size="22" /><h3>没有找到可导入的 Host</h3><p>请确认 ~/.ssh/config 中存在具体别名。</p></div>
        <div v-else class="import-list">
          <label v-for="candidate in candidates" :key="candidate.alias" class="import-row" :class="{ disabled: candidate.alreadyAdded }">
            <input type="checkbox" :checked="selected.includes(candidate.alias)" :disabled="candidate.alreadyAdded" @change="toggle(candidate.alias)" />
            <span class="import-name"><strong>{{ candidate.alias }}</strong><small>{{ candidate.user }}@{{ candidate.host }}:{{ candidate.port }}</small></span>
            <span class="tag">{{ candidate.alreadyAdded ? '已添加' : '可导入' }}</span>
          </label>
        </div>
        <p class="form-hint">导入不会自动连接；保存后请点击“刷新巡检”，首次连接会要求确认主机指纹。</p>
      </div>
      <footer class="modal-foot">
        <button class="button ghost" @click="emit('close')">取消</button>
        <button class="button primary" :disabled="!selected.length || importing" @click="doImport"><Icon name="download" :size="16" />{{ importing ? '正在导入…' : `导入 ${selected.length} 个` }}</button>
      </footer>
    </div>
  </div>
</template>
