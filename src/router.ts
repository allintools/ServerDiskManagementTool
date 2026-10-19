import { reactive } from 'vue'

export type PageName = 'fleet' | 'servers' | 'analysis' | 'tasks' | 'alerts' | 'settings'
export type ServerTab = 'overview' | 'disks' | 'usage' | 'activity'

export interface RouteState {
  page: PageName
  serverId: string | null
  tab: ServerTab
}

const pages: PageName[] = ['fleet', 'servers', 'analysis', 'tasks', 'alerts', 'settings']
const tabs: ServerTab[] = ['overview', 'disks', 'usage', 'activity']

function parse(hash: string): RouteState {
  const clean = hash.replace(/^#\/?/, '').replace(/\/+$/, '')
  const parts = clean.split('/').filter(Boolean)
  if (parts[0] === 'servers' && parts[1]) {
    return { page: 'servers', serverId: parts[1], tab: tabs.includes(parts[2] as ServerTab) ? parts[2] as ServerTab : 'overview' }
  }
  if (parts[0] === 'servers') return { page: 'servers', serverId: null, tab: 'overview' }
  const page = pages.includes(parts[0] as PageName) ? parts[0] as PageName : 'fleet'
  return { page, serverId: null, tab: 'overview' }
}

export const route = reactive<RouteState>(parse(location.hash))

function sync() {
  Object.assign(route, parse(location.hash))
}

window.addEventListener('hashchange', sync)

export function navigate(path: string) {
  const target = path.startsWith('#') ? path : `#${path.startsWith('/') ? path : `/${path}`}`
  if (location.hash === target) sync()
  else location.hash = target
}

export function openServer(id: string, tab: ServerTab = 'overview') {
  navigate(`/servers/${encodeURIComponent(id)}/${tab}`)
}

export function setServerTab(tab: ServerTab) {
  if (route.serverId) navigate(`/servers/${encodeURIComponent(route.serverId)}/${tab}`)
}
