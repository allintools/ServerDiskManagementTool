<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import type { KnownHost, Settings, SshDiagnostics } from '../types'
import { api, isTauri } from '../services/api'
import { appSettings, appState, loadAppInfo, saveSettings } from '../services/store'
import { notify, reportError } from '../services/ui'
import Icon from '../components/Icon.vue'
import EmptyState from '../components/EmptyState.vue'

const form = reactive<Settings>({ ...appSettings })
const saving = ref(false)
const diagnostics = ref<SshDiagnostics | null>(null)
const knownHosts = ref<KnownHost[]>([])
const busyHosts = ref(false)
const originalAllowDelete = appSettings.allowPermanentDelete

function syncForm() { Object.assign(form, appSettings) }

async function save() {
  saving.value = true
  try {
    await saveSettings({ ...form })
    await loadAppInfo()
    notify('设置已保存')
  } catch (error) { reportError(error) } finally { saving.value = false }
}

async function loadDiagnostics() {
  try { diagnostics.value = await api.diagnoseSsh() } catch (error) { reportError(error) }
}
async function loadHosts() {
  busyHosts.value = true
  try { knownHosts.value = await api.listKnownHosts() } catch (error) { reportError(error) } finally { busyHosts.value = false }
}
async function removeHost(host: KnownHost) {
  if (!window.confirm(`删除 ${host.host}:${host.port} 的 ${host.keyType} 信任？下次连接需要重新确认指纹。`)) return
  try { await api.removeKnownHost(host.id); await loadHosts(); notify('已删除主机密钥信任') } catch (error) { reportError(error) }
}

async function exportReport(format: 'csv' | 'json') {
  try {
    if (!isTauri) { notify('导出需要在桌面应用中运行', 'error'); return }
    const { save } = await import('@tauri-apps/plugin-dialog')
    const path = await save({ defaultPath: `server-disk-management-report.${format}`, filters: [{ name: format.toUpperCase(), extensions: [format] }] })
    if (!path) return
    const saved = await api.exportReport(path, format)
    notify(`已导出到 ${saved}`)
  } catch (error) { reportError(error) }
}

function resetDefaults() {
  Object.assign(form, {
    ...appSettings,
    thresholdWarn: 80, thresholdCritical: 90, inodeWarn: 85, inodeCritical: 95,
    concurrency: 6, cleanupConcurrency: 2, connectTimeoutSecs: 8, probeTimeoutSecs: 20,
    autoRefreshSecs: 60, quarantineDays: 7,
  })
}

onMounted(async () => {
  syncForm()
  await Promise.all([loadDiagnostics(), loadHosts()])
})
</script>

<template>
  <div class="page settings-page">
    <div class="action-row">
      <div class="action-buttons">
        <button class="button ghost" @click="resetDefaults">恢复默认</button>
        <button class="button primary" :disabled="saving" @click="save"><Icon name="check" :size="16" />{{ saving ? '正在保存…' : '保存设置' }}</button>
      </div>
    </div>

    <div class="settings-grid">
      <section class="panel settings-section">
        <header class="panel-head"><div><span class="section-kicker">APPEARANCE & REFRESH</span><h3>外观与刷新</h3></div></header>
        <label class="field"><span>主题</span><select v-model="form.theme"><option value="dark">深色</option><option value="light">浅色</option><option value="system">跟随系统</option></select></label>
        <label class="field"><span>自动刷新间隔（秒，0 为关闭）</span><input v-model.number="form.autoRefreshSecs" type="number" min="0" max="3600" /></label>
        <label class="field"><span>巡检并发（1–32）</span><input v-model.number="form.concurrency" type="number" min="1" max="32" /></label>
        <label class="field"><span>连接超时（秒）</span><input v-model.number="form.connectTimeoutSecs" type="number" min="2" max="60" /></label>
        <label class="field"><span>巡检超时（秒）</span><input v-model.number="form.probeTimeoutSecs" type="number" min="5" max="300" /></label>
      </section>

      <section class="panel settings-section">
        <header class="panel-head"><div><span class="section-kicker">ALERT THRESHOLDS</span><h3>告警阈值</h3></div></header>
        <label class="field"><span>容量警告（%）</span><input v-model.number="form.thresholdWarn" type="number" min="50" max="99" /></label>
        <label class="field"><span>容量严重（%）</span><input v-model.number="form.thresholdCritical" type="number" min="50" max="100" /></label>
        <label class="field"><span>inode 警告（%）</span><input v-model.number="form.inodeWarn" type="number" min="50" max="99" /></label>
        <label class="field"><span>inode 严重（%）</span><input v-model.number="form.inodeCritical" type="number" min="50" max="100" /></label>
        <p class="form-hint">阈值只影响告警评估，不自动清理。容量与 inode 同时检查。</p>
      </section>

      <section class="panel settings-section">
        <header class="panel-head"><div><span class="section-kicker">CLEANUP SAFETY</span><h3>空间回收</h3></div></header>
        <label class="field"><span>写任务并发（1–8）</span><input v-model.number="form.cleanupConcurrency" type="number" min="1" max="8" /></label>
        <label class="field"><span>计划有效期（分钟）</span><input v-model.number="form.cleanupExpiryMins" type="number" min="1" max="60" /></label>
        <label class="field"><span>隔离保留（天）</span><input v-model.number="form.quarantineDays" type="number" min="1" max="90" /></label>
        <label class="check-line danger-line"><input v-model="form.allowPermanentDelete" type="checkbox" />允许永久删除单个文件（不可恢复）</label>
        <p class="form-hint">默认关闭。即使开启，也只允许从大文件扫描结果中选中的单个普通文件；目录只能隔离。</p>
        <div v-if="form.allowPermanentDelete && !originalAllowDelete" class="notice danger compact"><Icon name="warning" :size="16" /><p>永久删除没有隔离区可恢复，执行前还需输入 DELETE 确认。</p></div>
      </section>

      <section class="panel settings-section">
        <header class="panel-head"><div><span class="section-kicker">RETENTION</span><h3>数据保留</h3></div></header>
        <label class="field"><span>快照保留（天）</span><input v-model.number="form.snapshotRetentionDays" type="number" min="1" max="365" /></label>
        <label class="field"><span>任务保留（天）</span><input v-model.number="form.taskRetentionDays" type="number" min="1" max="3650" /></label>
        <p class="form-hint">SQLite 仅保存在本机。启动和每日清理会按保留策略删除旧快照与任务。</p>
        <div class="export-actions"><button class="button ghost" @click="exportReport('csv')"><Icon name="download" :size="15" />导出 CSV</button><button class="button ghost" @click="exportReport('json')"><Icon name="download" :size="15" />导出 JSON</button></div>
      </section>
    </div>

    <section class="panel">
      <header class="panel-head"><div><span class="section-kicker">SSH DIAGNOSTICS</span><h3>SSH 与主机密钥</h3></div><button class="button ghost small" @click="loadDiagnostics"><Icon name="refresh" :size="14" />重新检测</button></header>
      <div v-if="diagnostics" class="diagnostics-grid">
        <div><span>SSH</span><code>{{ diagnostics.sshPath }}</code></div>
        <div><span>版本</span><code>{{ diagnostics.sshVersion }}</code></div>
        <div><span>agent socket</span><code>{{ diagnostics.agentSocket || '未检测到（可使用私钥+口令）' }}</code></div>
        <div><span>agent keys</span><code>{{ diagnostics.agentKeys.length ? diagnostics.agentKeys.join(' · ') : '无' }}</code></div>
        <div v-if="diagnostics.agentError"><span>agent 状态</span><code class="text-danger">{{ diagnostics.agentError }}</code></div>
        <div><span>known_hosts</span><code>{{ diagnostics.knownHostsPath }}</code></div>
        <div><span>数据目录</span><code>{{ diagnostics.dataDir }}</code></div>
      </div>
      <p v-if="diagnostics?.os === 'windows'" class="form-hint">Windows 使用系统 OpenSSH。密码/口令保存在 Windows 凭据管理器；SSH agent 认证需先启动 ssh-agent 服务并执行 ssh-add。</p>
      <p v-else class="form-hint">密码/口令保存在 macOS 钥匙串；agent 认证需要本机 ssh-agent 已加载密钥。</p>
      <div class="known-hosts">
        <div class="known-hosts-head"><strong>已信任主机密钥</strong><span>{{ knownHosts.length }} 条</span></div>
        <EmptyState v-if="!knownHosts.length" icon="key" title="还没有信任的主机密钥" description="首次连接服务器时会展示指纹并要求确认。" />
        <div v-else class="known-host-list">
          <div v-for="host in knownHosts" :key="host.id" class="known-host-row">
            <div><strong>{{ host.host }}:{{ host.port }}</strong><small>{{ host.keyType }} · {{ host.fingerprint }}</small></div>
            <button class="button ghost small danger-text" @click="removeHost(host)">删除信任</button>
          </div>
        </div>
      </div>
    </section>

    <section class="panel about-panel">
      <div class="about-mark"><Icon name="server" :size="22" /></div>
      <div><h3>服务器硬盘管理工具 {{ appState.appVersion || '0.1.0' }}</h3><p>ServerDiskManagementTool · 远程 Linux 磁盘巡检与受控空间回收。通过系统 OpenSSH 连接，不使用云端、不采集遥测。</p><p class="muted">数据目录：{{ appState.dataDir || '系统应用数据目录' }}</p></div>
      <span class="mode-pill"><i></i>{{ appState.demo ? '演示模式' : '原生模式' }}</span>
    </section>
  </div>
</template>
