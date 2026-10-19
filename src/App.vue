<script setup lang="ts">
import { computed, onMounted, onUnmounted, watch } from 'vue'
import Icon from './components/Icon.vue'
import FleetView from './views/FleetView.vue'
import ServersView from './views/ServersView.vue'
import AnalysisView from './views/AnalysisView.vue'
import TasksView from './views/TasksView.vue'
import AlertsView from './views/AlertsView.vue'
import SettingsView from './views/SettingsView.vue'
import ServerDetailView from './views/ServerDetailView.vue'
import { route, navigate, type PageName } from './router'
import { appSettings, appState, loadAppInfo, loadSettings } from './services/store'
import { api, isTauri } from './services/api'
import { reportError, ui } from './services/ui'

const pages: { id: PageName; title: string; subtitle: string; icon: string }[] = [
  { id: 'fleet', title: '机群总览', subtitle: '容量、风险与最近任务', icon: 'fleet' },
  { id: 'servers', title: '服务器', subtitle: '批量巡检与连接管理', icon: 'servers' },
  { id: 'analysis', title: '磁盘分析', subtitle: '跨机容量与空间线索', icon: 'analysis' },
  { id: 'tasks', title: '任务', subtitle: '逐主机结果与审计', icon: 'tasks' },
  { id: 'alerts', title: '告警', subtitle: '阈值、只读挂载与离线', icon: 'alerts' },
  { id: 'settings', title: '设置', subtitle: 'SSH、阈值与数据保留', icon: 'settings' },
]

const views = {
  fleet: FleetView, servers: ServersView, analysis: AnalysisView,
  tasks: TasksView, alerts: AlertsView, settings: SettingsView,
}

const activeNav = computed(() => (route.serverId ? 'servers' : route.page))
const detailServerId = computed(() => (route.page === 'servers' ? route.serverId : null))

function applyTheme() {
  const dark = appSettings.theme !== 'light'
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', dark ? '#0f1115' : '#f4f4f5')
}

watch(() => appSettings.theme, applyTheme, { immediate: true })

onMounted(async () => {
  ui.demo = appState.demo
  applyTheme()
  await Promise.all([loadSettings(), loadAppInfo()])
  applyTheme()
  if (isTauri) api.frontendReady().catch(() => undefined)
})

onUnmounted(() => undefined)

function go(id: PageName) {
  navigate(`/${id}`)
}
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <button class="brand" aria-label="回到机群总览" @click="go('fleet')">
        <span class="brand-mark" aria-hidden="true">
          <span></span><span></span><span></span>
        </span>
        <span class="brand-text"><strong>服务器硬盘管理工具</strong><small>ServerDiskManagementTool</small></span>
      </button>

      <div class="nav-section">工作区</div>
      <nav class="main-nav" aria-label="主要导航">
        <button v-for="page in pages" :key="page.id" class="nav-item" :class="{ active: activeNav === page.id }" :aria-current="activeNav === page.id ? 'page' : undefined" @click="go(page.id)">
          <Icon :name="page.icon" :size="18" />
          <span>{{ page.title }}</span>
          <i v-if="activeNav === page.id" class="nav-dot"></i>
        </button>
      </nav>

      <div class="sidebar-bottom">
        <div class="readonly-card">
          <Icon :name="'shield'" :size="15" />
          <div><strong>受控写操作</strong><small>清理必须经过计划、令牌与执行前重检</small></div>
        </div>
        <div class="sidebar-meta">
          <span>{{ appState.demo ? 'DEMO' : (isTauri ? 'NATIVE' : 'BROWSER') }}</span>
          <span>v{{ appState.appVersion || '0.1.0' }}</span>
        </div>
      </div>
    </aside>

    <section class="workspace">
      <header class="topbar">
        <div class="topbar-actions">
          <span class="mode-pill" :class="{ demo: appState.demo }"><i></i>{{ appState.demo ? '演示数据' : '实时 SSH' }}</span>
          <button class="icon-button" title="刷新设置" aria-label="刷新设置" @click="loadSettings().then(applyTheme).catch(reportError)"><Icon name="refresh" :size="17" /></button>
          <button class="icon-button" :title="appSettings.theme === 'light' ? '切换深色' : '切换浅色'" :aria-label="appSettings.theme === 'light' ? '切换深色' : '切换浅色'" @click="appSettings.theme = appSettings.theme === 'light' ? 'dark' : 'light'">
            <Icon :name="appSettings.theme === 'light' ? 'moon' : 'sun'" :size="17" />
          </button>
        </div>
      </header>

      <main class="workspace-scroll" id="main-content">
        <div v-if="appState.demo" class="demo-banner"><Icon name="help" :size="15" /><span>当前是浏览器演示模式，展示样例数据；真实 SSH 操作请在桌面应用中运行。</span></div>
        <ServerDetailView v-if="detailServerId" :key="detailServerId" :server-id="detailServerId" />
        <component :is="views[route.page]" v-else :key="route.page" />
      </main>

      <footer class="statusbar">
        <span class="status-left"><i class="live-dot" :class="{ busy: ui.busy }"></i>{{ ui.busy ? '正在执行…' : '就绪' }}<span class="divider"></span>{{ appSettings.concurrency }} 并发巡检</span>
        <span class="status-right"><span>{{ appState.sshVersion || 'OpenSSH' }}</span><span class="divider"></span><span>数据仅存本机</span><span class="divider"></span><span>{{ appState.demo ? '演示模式' : '只读 + 受控回收' }}</span></span>
      </footer>
    </section>

    <div v-if="ui.toast" class="toast" :class="{ error: ui.toastKind === 'error' }" :role="ui.toastKind === 'error' ? 'alert' : 'status'">
      <Icon :name="ui.toastKind === 'error' ? 'warning' : 'check'" :size="17" />{{ ui.toast }}
    </div>
  </div>
</template>
