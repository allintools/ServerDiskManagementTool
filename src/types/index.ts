export type AuthMode = 'agent' | 'key' | 'password'

export interface ServerSummary {
  os?: string | null
  distro?: string | null
  kernel?: string | null
  arch?: string | null
  uptimeSecs?: number | null
  cpuCount?: number | null
  memoryTotalBytes?: number | null
  load1?: number | null
  privilege?: string | null
  maxUsedPct?: number | null
  volumeCount: number
  readOnlyCount: number
  warningCount: number
  criticalCount: number
  collectedAt?: number | null
  latencyMs?: number | null
}

export interface Server {
  id: string
  name: string
  alias?: string | null
  host: string
  port: number
  user: string
  authMode: AuthMode
  keyPath?: string | null
  hasSecret: boolean
  groupName?: string | null
  tags: string[]
  enabled: boolean
  lastStatus: string
  lastSeenAt?: number | null
  lastError?: string | null
  summary: ServerSummary
  createdAt: number
  updatedAt: number
}

export interface DiskInfo {
  name: string
  model?: string | null
  serial?: string | null
  sizeBytes: number
  diskType: string
  rotational: boolean
  readOnly: boolean
}

export interface VolumeInfo {
  mount: string
  device: string
  fsType: string
  label?: string | null
  totalBytes: number
  usedBytes: number
  availableBytes: number
  usedPct: number
  inodeTotal?: number | null
  inodeUsed?: number | null
  inodePct?: number | null
  readOnly: boolean
  virtualFs: boolean
  options: string[]
}

export interface Capabilities {
  posixShell: boolean
  duZero: boolean
  gnuFind: boolean
  lsblk: boolean
  sudoNopass: boolean
  systemd: boolean
  journalctl: boolean
  docker: boolean
  timeout: boolean
  packageManagers: string[]
  uid?: number | null
  containerized: boolean
}

export interface Snapshot {
  id: string
  serverId: string
  collectedAt: number
  status: string
  durationMs: number
  error?: string | null
  maxUsedPct: number
  disks: DiskInfo[]
  volumes: VolumeInfo[]
  capabilities: Capabilities
  os?: string | null
  distro?: string | null
  kernel?: string | null
  arch?: string | null
  uptimeSecs?: number | null
  cpuCount?: number | null
  memoryTotalBytes?: number | null
  load1?: number | null
  privilege: string
}

export interface Task {
  id: string
  kind: string
  title: string
  status: string
  totalHosts: number
  succeeded: number
  failed: number
  partial: number
  cancelled: number
  concurrency: number
  startedAt: number
  finishedAt?: number | null
  planId?: string | null
  error?: string | null
}

export interface TaskHost {
  id: number
  serverId: string
  serverName: string
  status: string
  startedAt?: number | null
  finishedAt?: number | null
  durationMs?: number | null
  bytes: number
  errorCode?: string | null
  error?: string | null
  commandPreview?: string | null
  outputExcerpt?: string | null
  result?: unknown
}

export interface TaskDetail {
  task: Task
  hosts: TaskHost[]
}

export interface UsageScan {
  id: string
  serverId: string
  mount: string
  rootPath: string
  status: string
  startedAt: number
  finishedAt?: number | null
  totalBytes: number
  entries: number
  skipped: number
  partial: boolean
  error?: string | null
}

export interface UsageNode {
  id: number
  scanId: string
  parentId?: number | null
  name: string
  path: string
  kind: string
  sizeBytes: number
  depth: number
  childCount: number
  expandedAt?: number | null
}

export interface LargeFile {
  id: number
  scanId: string
  path: string
  sizeBytes: number
  mtime?: number | null
}

export interface Alert {
  id: string
  serverId: string
  serverName: string
  kind: string
  severity: string
  title: string
  detail: string
  firstSeenAt: number
  lastSeenAt: number
  resolvedAt?: number | null
  acknowledgedAt?: number | null
}

export interface Settings {
  theme: string
  concurrency: number
  connectTimeoutSecs: number
  probeTimeoutSecs: number
  cleanupConcurrency: number
  cleanupExpiryMins: number
  snapshotRetentionDays: number
  taskRetentionDays: number
  thresholdWarn: number
  thresholdCritical: number
  inodeWarn: number
  inodeCritical: number
  autoRefreshSecs: number
  quarantineDays: number
  allowPermanentDelete: boolean
}

export interface KnownHost {
  id: number
  host: string
  port: number
  keyType: string
  fingerprint: string
  createdAt: number
}

export interface HostFingerprint {
  keyType: string
  fingerprint: string
}

export interface HostKeyChallenge {
  pendingId: string
  host: string
  port: number
  fingerprints: HostFingerprint[]
  message: string
}

export interface ConnectionTest {
  status: string
  message: string
  latencyMs?: number | null
  hostKey?: HostKeyChallenge | null
  serverId?: string | null
}

export interface ImportCandidate {
  alias: string
  host: string
  user: string
  port: number
  identityFile?: string | null
  alreadyAdded: boolean
}

export interface ReclaimCandidate {
  serverId: string
  serverName: string
  templateId: string
  title: string
  description: string
  sizeBytes: number
  risk: string
  reversible: boolean
  commandPreview: string
  available: boolean
}

export interface FileCleanupRequest {
  scanId: string
  nodeId?: number | null
  largeFileId?: number | null
  action: 'quarantine' | 'delete'
}

export interface CleanupRequest {
  serverIds: string[]
  templateIds: string[]
  files: FileCleanupRequest[]
}

export interface CleanupPlanItem {
  id: number
  planId: string
  serverId: string
  serverName: string
  kind: string
  templateId?: string | null
  path?: string | null
  displayPath?: string | null
  quarantinePath?: string | null
  sizeBytes: number
  risk: string
  reversible: boolean
  commandPreview: string
  defaultSelected: boolean
  selected: boolean
  status: string
  error?: string | null
}

export interface CleanupPlan {
  id: string
  token: string
  status: string
  createdAt: number
  expiresAt: number
  hostCount: number
  itemCount: number
  totalBytes: number
  irreversible: number
  items: CleanupPlanItem[]
}

export interface QuarantineEntry {
  id: number
  serverId: string
  serverName: string
  originalPath: string
  quarantinePath: string
  sizeBytes: number
  createdAt: number
  expiresAt?: number | null
  status: string
}

export interface CleanupExecution {
  taskId: string
  planId: string
  status: string
}

export interface ActionResult {
  ok: boolean
  message: string
  taskId?: string | null
}

export interface AppInfo {
  version: string
  product: string
  dataDir: string
  knownHostsPath: string
  sshPath: string
  sshVersion: string
}

export interface SshDiagnostics {
  sshPath: string
  sshVersion: string
  agentSocket?: string | null
  agentKeys: string[]
  agentError?: string | null
  knownHostsPath: string
  dataDir: string
  appVersion: string
  os: string
}

export interface MetricSample {
  collectedAt: number
  cpuPercent: number
  memoryUsedBytes: number
  memoryTotalBytes: number
  load1: number
  disks: DiskMetric[]
}

export interface DiskMetric {
  name: string
  readBytesPerSec: number
  writeBytesPerSec: number
  readIops: number
  writeIops: number
}

export interface ServerInput {
  name: string
  alias?: string | null
  host: string
  port: number
  user: string
  authMode: AuthMode
  keyPath?: string | null
  groupName?: string | null
  tags: string[]
  secret?: string | null
  rememberSecret: boolean
}

export interface ServerUpdate extends ServerInput {
  id: string
  clearSecret: boolean
}

export interface ApiErrorShape {
  code?: string
  message?: string
  detail?: string | null
  retryable?: boolean
}
