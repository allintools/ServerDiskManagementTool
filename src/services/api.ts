import { invoke } from '@tauri-apps/api/core'
import type {
  ActionResult, Alert, AppInfo, CleanupExecution, CleanupPlan, CleanupRequest, ConnectionTest,
  ImportCandidate, KnownHost, LargeFile, MetricSample, QuarantineEntry, ReclaimCandidate, Server,
  ServerInput, ServerUpdate, Settings, Snapshot, SshDiagnostics, Task, TaskDetail, UsageNode,
  UsageScan,
} from '../types'
import { demoApi } from './demo'

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
export const isDemo = !isTauri || import.meta.env.VITE_DEMO === '1'

export const DEFAULT_SETTINGS: Settings = {
  theme: 'dark',
  concurrency: 6,
  connectTimeoutSecs: 8,
  probeTimeoutSecs: 20,
  cleanupConcurrency: 2,
  cleanupExpiryMins: 5,
  snapshotRetentionDays: 30,
  taskRetentionDays: 90,
  thresholdWarn: 80,
  thresholdCritical: 90,
  inodeWarn: 85,
  inodeCritical: 95,
  autoRefreshSecs: 60,
  quarantineDays: 7,
  allowPermanentDelete: false,
}

const NOT_DESKTOP = '此功能需要在“服务器硬盘管理工具”桌面应用中运行。'

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) throw new Error(NOT_DESKTOP)
  return invoke<T>(cmd, args)
}

export interface RackApi {
  getAppInfo: () => Promise<AppInfo>
  diagnoseSsh: () => Promise<SshDiagnostics>
  listServers: () => Promise<Server[]>
  getServer: (serverId: string) => Promise<Server>
  createServer: (input: ServerInput) => Promise<Server>
  updateServer: (input: ServerUpdate) => Promise<Server>
  deleteServer: (serverId: string) => Promise<void>
  importSshConfig: () => Promise<ImportCandidate[]>
  testConnection: (serverId: string, secret?: string) => Promise<ConnectionTest>
  trustHostKey: (pendingId: string) => Promise<KnownHost[]>
  listKnownHosts: () => Promise<KnownHost[]>
  removeKnownHost: (id: number) => Promise<void>
  getLatestSnapshot: (serverId: string) => Promise<Snapshot | null>
  listSnapshots: (serverId: string, limit?: number) => Promise<Snapshot[]>
  startBatchProbe: (serverIds: string[]) => Promise<Task>
  getTask: (taskId: string) => Promise<TaskDetail>
  listTasks: (limit?: number) => Promise<Task[]>
  cancelTask: (taskId: string) => Promise<void>
  startUsageScan: (serverId: string, mount: string) => Promise<UsageScan>
  getUsageScan: (scanId: string) => Promise<UsageScan>
  latestUsageScan: (serverId: string) => Promise<UsageScan | null>
  cancelUsageScan: (scanId: string) => Promise<void>
  usageRoot: (scanId: string) => Promise<UsageNode | null>
  listUsageChildren: (scanId: string, parentId: number) => Promise<UsageNode[]>
  expandUsageNode: (nodeId: number) => Promise<UsageNode[]>
  scanLargeFiles: (scanId: string, minBytes: number) => Promise<LargeFile[]>
  listLargeFiles: (scanId: string) => Promise<LargeFile[]>
  getRecommendations: (serverId: string) => Promise<ReclaimCandidate[]>
  sampleMetrics: (serverId: string) => Promise<MetricSample>
  listAlerts: (activeOnly?: boolean, limit?: number) => Promise<Alert[]>
  acknowledgeAlert: (alertId: string) => Promise<void>
  resolveAlert: (alertId: string) => Promise<void>
  prepareCleanupPlan: (request: CleanupRequest) => Promise<CleanupPlan>
  executeCleanupPlan: (planId: string, token: string) => Promise<CleanupExecution>
  getCleanupPlan: (planId: string) => Promise<CleanupPlan>
  listQuarantine: (serverId?: string | null) => Promise<QuarantineEntry[]>
  restoreQuarantine: (entryId: number) => Promise<ActionResult>
  purgeQuarantine: (entryId: number) => Promise<ActionResult>
  getSettings: () => Promise<Settings>
  saveSettings: (settings: Settings) => Promise<Settings>
  exportReport: (path: string, format: string) => Promise<string>
  listAudit: (limit?: number) => Promise<unknown[]>
  frontendReady: () => Promise<string>
}

const realApi: RackApi = {
  getAppInfo: () => call<AppInfo>('get_app_info'),
  diagnoseSsh: () => call<SshDiagnostics>('diagnose_ssh'),
  listServers: () => call<Server[]>('list_servers'),
  getServer: (serverId) => call<Server>('get_server', { serverId }),
  createServer: (input) => call<Server>('create_server', { input }),
  updateServer: (input) => call<Server>('update_server', { input }),
  deleteServer: (serverId) => call<void>('delete_server', { serverId }),
  importSshConfig: () => call<ImportCandidate[]>('import_ssh_config'),
  testConnection: (serverId, secret) => call<ConnectionTest>('test_connection', { serverId, secret: secret ?? null }),
  trustHostKey: (pendingId) => call<KnownHost[]>('trust_host_key', { pendingId }),
  listKnownHosts: () => call<KnownHost[]>('list_known_hosts'),
  removeKnownHost: (id) => call<void>('remove_known_host', { id }),
  getLatestSnapshot: (serverId) => call<Snapshot | null>('get_latest_snapshot', { serverId }),
  listSnapshots: (serverId, limit = 30) => call<Snapshot[]>('list_snapshots', { serverId, limit }),
  startBatchProbe: (serverIds) => call<Task>('start_batch_probe', { serverIds }),
  getTask: (taskId) => call<TaskDetail>('get_task', { taskId }),
  listTasks: (limit = 100) => call<Task[]>('list_tasks', { limit }),
  cancelTask: (taskId) => call<void>('cancel_task', { taskId }),
  startUsageScan: (serverId, mount) => call<UsageScan>('start_usage_scan', { serverId, mount }),
  getUsageScan: (scanId) => call<UsageScan>('get_usage_scan', { scanId }),
  latestUsageScan: (serverId) => call<UsageScan | null>('latest_usage_scan', { serverId }),
  cancelUsageScan: (scanId) => call<void>('cancel_usage_scan', { scanId }),
  usageRoot: (scanId) => call<UsageNode | null>('usage_root', { scanId }),
  listUsageChildren: (scanId, parentId) => call<UsageNode[]>('list_usage_children', { scanId, parentId }),
  expandUsageNode: (nodeId) => call<UsageNode[]>('expand_usage_node', { nodeId }),
  scanLargeFiles: (scanId, minBytes) => call<LargeFile[]>('scan_large_files', { scanId, minBytes }),
  listLargeFiles: (scanId) => call<LargeFile[]>('list_large_files', { scanId }),
  getRecommendations: (serverId) => call<ReclaimCandidate[]>('get_recommendations', { serverId }),
  sampleMetrics: (serverId) => call<MetricSample>('sample_metrics', { serverId }),
  listAlerts: (activeOnly = true, limit = 200) => call<Alert[]>('list_alerts', { activeOnly, limit }),
  acknowledgeAlert: (alertId) => call<void>('acknowledge_alert', { alertId }),
  resolveAlert: (alertId) => call<void>('resolve_alert', { alertId }),
  prepareCleanupPlan: (request) => call<CleanupPlan>('prepare_cleanup_plan', { request }),
  executeCleanupPlan: (planId, token) => call<CleanupExecution>('execute_cleanup_plan', { planId, token }),
  getCleanupPlan: (planId) => call<CleanupPlan>('get_cleanup_plan', { planId }),
  listQuarantine: (serverId) => call<QuarantineEntry[]>('list_quarantine', { serverId: serverId ?? null }),
  restoreQuarantine: (entryId) => call<ActionResult>('restore_quarantine', { entryId }),
  purgeQuarantine: (entryId) => call<ActionResult>('purge_quarantine', { entryId }),
  getSettings: () => call<Settings>('get_settings'),
  saveSettings: (settings) => call<Settings>('save_settings', { settings }),
  exportReport: (path, format) => call<string>('export_report', { path, format }),
  listAudit: (limit = 100) => call<unknown[]>('list_audit', { limit }),
  frontendReady: () => call<string>('frontend_ready'),
}

export const api: RackApi = isDemo ? demoApi : realApi
export const demoMode = isDemo
