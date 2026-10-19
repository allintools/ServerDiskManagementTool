import { reactive } from 'vue'
import { api, DEFAULT_SETTINGS, demoMode } from './api'
import type { Settings } from '../types'

export const appSettings = reactive<Settings>({ ...DEFAULT_SETTINGS })
export const appState = reactive({
  loaded: false,
  appVersion: '',
  sshVersion: '',
  dataDir: '',
  knownHostsPath: '',
  demo: demoMode,
  lastRefresh: 0,
})

export async function loadSettings() {
  try {
    const loaded = await api.getSettings()
    Object.assign(appSettings, loaded)
  } catch {
    Object.assign(appSettings, DEFAULT_SETTINGS)
  }
  appState.loaded = true
}

export async function saveSettings(next: Settings) {
  const saved = await api.saveSettings(next)
  Object.assign(appSettings, saved)
  return saved
}

export function updateLocalSettings(patch: Partial<Settings>) {
  Object.assign(appSettings, patch)
}

export async function loadAppInfo() {
  try {
    const info = await api.getAppInfo()
    appState.appVersion = info.version
    appState.sshVersion = info.sshVersion
    appState.dataDir = info.dataDir
    appState.knownHostsPath = info.knownHostsPath
  } catch {
    // Browser demo keeps empty info.
  }
}
