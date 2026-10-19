<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { Alert } from '../types'
import { api } from '../services/api'
import { appSettings } from '../services/store'
import { formatTime, severityText, timeAgo } from '../services/format'
import { notify, reportError } from '../services/ui'
import { navigate, openServer } from '../router'
import Icon from '../components/Icon.vue'
import StatusPill from '../components/StatusPill.vue'
import EmptyState from '../components/EmptyState.vue'

const alerts = ref<Alert[]>([])
const activeOnly = ref(true)
const severity = ref('')
const loading = ref(true)
const workingId = ref('')

const filtered = computed(() => alerts.value.filter(a => !severity.value || a.severity === severity.value))
const counts = computed(() => ({
  critical: alerts.value.filter(a => a.severity === 'critical' && !a.resolvedAt).length,
  warning: alerts.value.filter(a => a.severity === 'warning' && !a.resolvedAt).length,
}))

async function load() {
  loading.value = true
  try { alerts.value = await api.listAlerts(activeOnly.value, 500) } catch (error) { reportError(error) } finally { loading.value = false }
}

async function ack(alert: Alert) {
  workingId.value = alert.id
  try { await api.acknowledgeAlert(alert.id); await load(); notify('已确认告警') } catch (error) { reportError(error) } finally { workingId.value = '' }
}
async function resolve(alert: Alert) {
  workingId.value = alert.id
  try { await api.resolveAlert(alert.id); await load(); notify('已手动标记恢复') } catch (error) { reportError(error) } finally { workingId.value = '' }
}

onMounted(load)
</script>

<template>
  <div class="page alerts-page">
    <div class="action-row">
      <div class="action-buttons">
        <button class="button ghost" @click="navigate('/settings')"><Icon name="settings" :size="16" />阈值设置</button>
        <button class="button ghost" @click="load"><Icon name="refresh" :size="16" />刷新</button>
      </div>
      <div class="toolbar-filters">
        <label class="check-line compact"><input v-model="activeOnly" type="checkbox" @change="load" />仅显示活动告警</label>
        <select v-model="severity" aria-label="严重级别"><option value="">全部级别</option><option value="critical">严重</option><option value="warning">警告</option><option value="info">提示</option></select>
        <span class="filter-count">{{ filtered.length }} 条</span>
      </div>
    </div>

    <div class="kpi-grid three">
      <article class="kpi-card"><span class="kpi-icon danger"><Icon name="alerts" :size="19" /></span><div><small>严重</small><strong>{{ counts.critical }}</strong><p>容量、inode 或离线</p></div></article>
      <article class="kpi-card"><span class="kpi-icon amber"><Icon name="warning" :size="19" /></span><div><small>警告</small><strong>{{ counts.warning }}</strong><p>接近阈值或只读挂载</p></div></article>
      <article class="kpi-card"><span class="kpi-icon blue"><Icon name="settings" :size="19" /></span><div><small>当前阈值</small><strong>{{ appSettings.thresholdWarn }}% / {{ appSettings.thresholdCritical }}%</strong><p>inode {{ appSettings.inodeWarn }}% / {{ appSettings.inodeCritical }}%</p></div></article>
    </div>

    <section class="panel">
      <div v-if="loading" class="inline-loading"><Icon name="loader" :size="17" class="spin" />加载告警…</div>
      <EmptyState v-else-if="!filtered.length" icon="shield" title="当前没有告警" description="巡检完成后会根据阈值自动评估容量、inode 与只读挂载。" />
      <div v-else class="alert-list">
        <article v-for="alert in filtered" :key="alert.id" class="alert-row" :class="[`severity-${alert.severity}`, { resolved: alert.resolvedAt }]">
          <span class="alert-severity"><Icon :name="alert.severity === 'critical' ? 'warning' : 'alerts'" :size="18" />{{ severityText(alert.severity) }}</span>
          <div class="alert-main">
            <button class="text-button alert-server" @click="openServer(alert.serverId, 'activity')">{{ alert.serverName }}</button>
            <strong>{{ alert.title }}</strong>
            <p>{{ alert.detail }}</p>
            <small>首次 {{ formatTime(alert.firstSeenAt) }} · 最近 {{ timeAgo(alert.lastSeenAt) }}<span v-if="alert.acknowledgedAt"> · 已确认</span><span v-if="alert.resolvedAt"> · 已恢复</span></small>
          </div>
          <div class="alert-actions">
            <button v-if="!alert.acknowledgedAt && !alert.resolvedAt" class="button ghost small" :disabled="workingId === alert.id" @click="ack(alert)">确认</button>
            <button v-if="!alert.resolvedAt" class="button ghost small" :disabled="workingId === alert.id" @click="resolve(alert)">标记恢复</button>
          </div>
        </article>
      </div>
    </section>
  </div>
</template>
