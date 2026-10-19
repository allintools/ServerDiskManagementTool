<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { CleanupPlan } from '../types'
import { api } from '../services/api'
import { formatBytes, formatTime, riskText } from '../services/format'
import { reportError } from '../services/ui'
import Icon from './Icon.vue'

const props = defineProps<{ open: boolean; plan: CleanupPlan | null }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'executed', taskId: string): void }>()
const confirmed = ref(false)
const typed = ref('')
const executing = ref(false)
const error = ref('')
const needsTyping = computed(() => !!props.plan?.items.some(i => i.kind === 'delete_file'))
const canExecute = computed(() => confirmed.value && (!needsTyping.value || typed.value.trim() === 'DELETE'))

watch(() => props.open, open => { if (open) { confirmed.value = false; typed.value = ''; error.value = ''; executing.value = false } })

async function execute() {
  if (!props.plan || !canExecute.value || executing.value) return
  executing.value = true
  error.value = ''
  try {
    const result = await api.executeCleanupPlan(props.plan.id, props.plan.token)
    emit('executed', result.taskId)
    emit('close')
  } catch (err) {
    error.value = (err as { message?: string })?.message ?? String(err)
  } finally {
    executing.value = false
  }
}
</script>

<template>
  <div v-if="open && plan" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal cleanup-modal" role="dialog" aria-modal="true" aria-labelledby="cleanup-title">
      <header class="modal-head">
        <div><span class="eyebrow">ONE-TIME PLAN · {{ plan.id.slice(0, 8) }}</span><h2 id="cleanup-title">确认空间回收计划</h2></div>
        <button class="icon-button" aria-label="关闭" @click="emit('close')"><Icon name="x" :size="18" /></button>
      </header>
      <div class="modal-body">
        <div class="plan-summary">
          <div><span>主机</span><strong>{{ plan.hostCount }}</strong></div>
          <div><span>清理项</span><strong>{{ plan.itemCount }}</strong></div>
          <div><span>预计释放</span><strong>{{ formatBytes(plan.totalBytes) }}</strong></div>
          <div><span>不可逆项</span><strong :class="{ 'text-danger': plan.irreversible }">{{ plan.irreversible }}</strong></div>
        </div>
        <p class="muted">计划将在 {{ formatTime(plan.expiresAt) }} 过期。执行前会重新检查目标、权限、挂载状态与工具；失败不会回滚，逐主机结果会写入审计。</p>
        <div class="plan-items">
          <div v-for="item in plan.items" :key="item.id" class="plan-item">
            <div class="plan-item-main">
              <div><strong>{{ item.displayPath || item.templateId }}</strong><small>{{ item.serverName }} · {{ item.kind === 'template' ? '模板回收' : item.kind === 'quarantine' ? '移动到隔离区' : '永久删除文件' }}</small></div>
              <div class="plan-item-side"><span class="tag" :class="`risk-${item.risk}`">{{ riskText(item.risk) }}</span><strong>{{ formatBytes(item.sizeBytes) }}</strong></div>
            </div>
            <code class="command-preview">{{ item.commandPreview }}</code>
          </div>
        </div>
        <div v-if="plan.irreversible" class="notice danger"><Icon name="warning" :size="17" /><p>模板回收与永久删除不可恢复；只有“移动到隔离区”可在保留期内恢复。请确认计划范围。</p></div>
        <label class="check-line"><input v-model="confirmed" type="checkbox" />我已核对主机、路径和命令，理解不可逆操作。</label>
        <label v-if="needsTyping" class="field"><span>输入 DELETE 以确认永久删除</span><input v-model="typed" placeholder="DELETE" autocomplete="off" /></label>
        <p v-if="error" class="form-error">{{ error }}</p>
      </div>
      <footer class="modal-foot">
        <button class="button ghost" @click="emit('close')">取消</button>
        <button class="button danger" :disabled="!canExecute || executing" @click="execute"><Icon name="trash" :size="16" />{{ executing ? '正在执行…' : '确认执行' }}</button>
      </footer>
    </div>
  </div>
</template>

