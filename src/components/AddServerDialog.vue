<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import { api } from '../services/api'
import type { Server, ServerInput } from '../types'
import Icon from './Icon.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'created', server: Server): void }>()

const form = reactive<ServerInput>({
  name: '', alias: '', host: '', port: 22, user: 'root', authMode: 'agent',
  keyPath: '', groupName: '', tags: [], secret: '', rememberSecret: true,
})
const tagText = ref('')
const saving = ref(false)
const error = ref('')

watch(() => props.open, open => {
  if (!open) return
  Object.assign(form, { name: '', alias: '', host: '', port: 22, user: 'root', authMode: 'agent', keyPath: '', groupName: '', tags: [], secret: '', rememberSecret: true })
  tagText.value = ''
  error.value = ''
})

async function submit() {
  if (saving.value) return
  if (!form.name.trim() || !form.host.trim() || !form.user.trim()) {
    error.value = '请填写名称、主机和用户名'
    return
  }
  saving.value = true
  error.value = ''
  try {
    const server = await api.createServer({
      ...form,
      alias: form.alias?.trim() || null,
      keyPath: form.keyPath?.trim() || null,
      groupName: form.groupName?.trim() || null,
      tags: tagText.value.split(/[,，\s]+/).map(t => t.trim()).filter(Boolean),
      secret: form.secret?.trim() || null,
    })
    emit('created', server)
    emit('close')
  } catch (err) {
    error.value = (err as { message?: string })?.message ?? String(err)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal add-server-modal" role="dialog" aria-modal="true" aria-labelledby="add-server-title">
      <header class="modal-head">
        <div><span class="eyebrow">SSH CONNECTION</span><h2 id="add-server-title">添加服务器</h2></div>
        <button class="icon-button" aria-label="关闭" @click="emit('close')"><Icon name="x" :size="18" /></button>
      </header>
      <div class="modal-body form-grid">
        <label class="field"><span>显示名称</span><input v-model="form.name" placeholder="web-01" autofocus /></label>
        <label class="field"><span>分组</span><input v-model="form.groupName" placeholder="生产 / 构建 / 存储" /></label>
        <label class="field span-2"><span>SSH 别名（可选）</span><input v-model="form.alias" placeholder="prod-web，填写后使用 ~/.ssh/config 中的配置" /></label>
        <label class="field"><span>主机地址</span><input v-model="form.host" placeholder="10.0.1.11 或别名" /></label>
        <label class="field"><span>端口</span><input v-model.number="form.port" type="number" min="1" max="65535" /></label>
        <label class="field"><span>用户名</span><input v-model="form.user" placeholder="deploy" /></label>
        <label class="field"><span>认证方式</span>
          <select v-model="form.authMode">
            <option value="agent">SSH agent（推荐）</option>
            <option value="key">指定私钥</option>
            <option value="password">密码</option>
          </select>
        </label>
        <label v-if="form.authMode === 'key'" class="field span-2"><span>私钥路径</span><input v-model="form.keyPath" placeholder="~/.ssh/id_ed25519" /></label>
        <label v-if="form.authMode === 'key'" class="field span-2"><span>私钥口令（可选）</span><input v-model="form.secret" type="password" placeholder="保存在系统凭据存储" /></label>
        <label v-if="form.authMode === 'password'" class="field span-2"><span>SSH 密码</span><input v-model="form.secret" type="password" placeholder="保存在系统凭据存储" /></label>
        <label class="field span-2"><span>标签</span><input v-model="tagText" placeholder="web, nginx, prod" /></label>
        <label class="check-line span-2"><input v-model="form.rememberSecret" type="checkbox" />将密码/口令保存到系统凭据存储；不保存则仅本次会话有效</label>
        <p v-if="error" class="form-error span-2">{{ error }}</p>
        <p class="form-hint span-2">主机密钥将在首次连接时展示指纹，确认后才会写入应用私有 known_hosts。密码和口令不会写入 SQLite。</p>
      </div>
      <footer class="modal-foot">
        <button class="button ghost" @click="emit('close')">取消</button>
        <button class="button primary" :disabled="saving" @click="submit"><Icon name="plus" :size="16" />{{ saving ? '正在保存…' : '保存并测试连接' }}</button>
      </footer>
    </div>
  </div>
</template>
