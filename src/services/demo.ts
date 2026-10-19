import type {
  ActionResult, Alert, AppInfo, CleanupExecution, CleanupPlan, CleanupRequest, ConnectionTest,
  ImportCandidate, KnownHost, LargeFile, MetricSample, QuarantineEntry, ReclaimCandidate, Server,
  ServerInput, ServerUpdate, Settings, Snapshot, SshDiagnostics, Task, TaskDetail, UsageNode,
  UsageScan,
} from '../types'
import type { RackApi } from './api'

const now = Date.now()
const HOUR = 3600_000
const GB = 1024 ** 3

const demoIsWindows = typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent)
const demoDataDir = demoIsWindows
  ? String.raw`%LOCALAPPDATA%\com.novarack.desktop`
  : '~/Library/Application Support/com.novarack.desktop'
const demoKnownHostsPath = demoIsWindows
  ? String.raw`%LOCALAPPDATA%\com.novarack.desktop\known_hosts`
  : '~/Library/Application Support/com.novarack.desktop/known_hosts'
const demoSshPath = demoIsWindows
  ? String.raw`C:\Windows\System32\OpenSSH\ssh.exe`
  : '/usr/bin/ssh'
const demoAgentSocket = demoIsWindows
  ? String.raw`\\.\pipe\openssh-ssh-agent`
  : '/private/tmp/com.apple.launchd.demo/Listeners'

const caps = {
  posixShell: true, duZero: true, gnuFind: true, lsblk: true, sudoNopass: true,
  systemd: true, journalctl: true, docker: true, timeout: true,
  packageManagers: ['apt-get'], uid: 1000, containerized: false,
}

function volume(mount: string, total: number, usedPct: number, fsType = 'ext4', readOnly = false, inodePct = 22): Snapshot['volumes'][number] {
  const totalBytes = Math.round(total * GB)
  const usedBytes = Math.round(totalBytes * usedPct / 100)
  return {
    mount, device: `/dev/vd${mount === '/' ? 'a1' : 'b1'}`, fsType, label: null,
    totalBytes, usedBytes, availableBytes: totalBytes - usedBytes, usedPct,
    inodeTotal: 6553600, inodeUsed: Math.round(6553600 * inodePct / 100), inodePct,
    readOnly, virtualFs: false, options: readOnly ? ['ro', 'relatime'] : ['rw', 'relatime'],
  }
}

function summary(server: Partial<Server>, maxUsed: number, volumes: number, warnings: number, critical: number): Server {
  const base = {
    id: 'srv-1', name: 'web-01', alias: null, host: '10.0.1.11', port: 22, user: 'deploy',
    authMode: 'agent' as const, keyPath: null, hasSecret: false, groupName: '生产', tags: ['web'],
    enabled: true, lastStatus: 'online', lastSeenAt: now - 4 * 60_000, lastError: null,
    createdAt: now - 30 * 24 * HOUR, updatedAt: now - 4 * 60_000,
  }
  return {
    ...base, ...server,
    summary: {
      os: 'Linux', distro: 'Debian GNU/Linux 12', kernel: '6.1.0-18-amd64', arch: 'x86_64',
      uptimeSecs: 987654, cpuCount: 8, memoryTotalBytes: 16 * GB, load1: 0.42,
      privilege: 'sudo', maxUsedPct: maxUsed, volumeCount: volumes, readOnlyCount: 0,
      warningCount: warnings, criticalCount: critical, collectedAt: now - 4 * 60_000, latencyMs: 86,
      ...(server.summary ?? {}),
    },
  } as Server
}

const servers: Server[] = [
  summary({ id: 'srv-1', name: 'web-01', host: '10.0.1.11', groupName: '生产', tags: ['web', 'nginx'] }, 82.4, 3, 1, 0),
  summary({ id: 'srv-2', name: 'web-02', host: '10.0.1.12', groupName: '生产', tags: ['web', 'nginx'] }, 64.1, 3, 0, 0),
  summary({ id: 'srv-3', name: 'db-01', host: '10.0.2.21', user: 'dba', groupName: '生产', tags: ['db', 'pg'], lastStatus: 'offline', lastError: '连接超时' }, 91.8, 4, 0, 1),
  summary({ id: 'srv-4', name: 'build-01', host: '10.0.3.31', user: 'builder', groupName: '构建', tags: ['ci'], lastStatus: 'online' }, 47.3, 2, 0, 0),
  summary({ id: 'srv-5', name: 'backup-01', host: '10.0.4.41', user: 'backup', groupName: '存储', tags: ['backup'], lastStatus: 'degraded' }, 76.2, 5, 1, 0),
]

const snapshotMap: Record<string, Snapshot> = {}
for (const server of servers) {
  const max = server.summary.maxUsedPct ?? 50
  snapshotMap[server.id] = {
    id: `snap-${server.id}`, serverId: server.id, collectedAt: server.summary.collectedAt ?? now,
    status: server.lastStatus === 'degraded' ? 'partial' : 'ok', durationMs: 620,
    error: server.lastStatus === 'degraded' ? '部分路径权限不足' : null, maxUsedPct: max,
    disks: [
      { name: 'vda', model: 'QEMU HARDDISK', serial: 'demo-001', sizeBytes: 100 * GB, diskType: 'ssd', rotational: false, readOnly: false },
      ...(server.id === 'srv-3' ? [{ name: 'vdb', model: 'Samsung SSD', serial: 'demo-002', sizeBytes: 2 * 1024 * GB, diskType: 'nvme', rotational: false, readOnly: false }] : []),
    ],
    volumes: [
      volume('/', 40, Math.min(max, 61), 'ext4', false, 18),
      volume('/data', 100, max, 'xfs', false, 27),
      ...(server.id === 'srv-5' ? [volume('/backup', 200, 76.2, 'ext4', true, 9)] : []),
    ],
    capabilities: caps,
    os: 'Linux', distro: 'Debian GNU/Linux 12', kernel: '6.1.0-18-amd64', arch: 'x86_64',
    uptimeSecs: 987654, cpuCount: 8, memoryTotalBytes: 16 * GB, load1: 0.42, privilege: 'sudo',
  }
}

const tasks: Task[] = [
  {
    id: 'task-demo-1', kind: 'probe', title: '批量巡检 5 台服务器', status: 'partial',
    totalHosts: 5, succeeded: 3, failed: 1, partial: 1, cancelled: 0, concurrency: 4,
    startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, planId: null, error: null,
  },
]
const taskHosts: Record<string, TaskDetail['hosts']> = {
  'task-demo-1': [
    { id: 1, serverId: 'srv-1', serverName: 'web-01', status: 'succeeded', startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, durationMs: 820, bytes: 0, errorCode: null, error: null, commandPreview: 'ssh web-01 probe_v1.sh', outputExcerpt: null, result: { volumes: 3 } },
    { id: 2, serverId: 'srv-2', serverName: 'web-02', status: 'succeeded', startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, durationMs: 710, bytes: 0, errorCode: null, error: null, commandPreview: 'ssh web-02 probe_v1.sh', outputExcerpt: null, result: { volumes: 3 } },
    { id: 3, serverId: 'srv-3', serverName: 'db-01', status: 'failed', startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, durationMs: 15000, bytes: 0, errorCode: 'CONNECT_TIMEOUT', error: '连接超时', commandPreview: 'ssh db-01 probe_v1.sh', outputExcerpt: 'connect to host 10.0.2.21 port 22: Operation timed out', result: null },
    { id: 4, serverId: 'srv-4', serverName: 'build-01', status: 'succeeded', startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, durationMs: 640, bytes: 0, errorCode: null, error: null, commandPreview: 'ssh build-01 probe_v1.sh', outputExcerpt: null, result: { volumes: 2 } },
    { id: 5, serverId: 'srv-5', serverName: 'backup-01', status: 'partial', startedAt: now - 8 * 60_000, finishedAt: now - 7 * 60_000, durationMs: 940, bytes: 0, errorCode: null, error: '部分路径权限不足', commandPreview: 'ssh backup-01 probe_v1.sh', outputExcerpt: '部分路径权限不足', result: { volumes: 5 } },
  ],
}

let alerts: Alert[] = [
  { id: 'al-1', serverId: 'srv-3', serverName: 'db-01', kind: 'volume_full', severity: 'critical', title: '/data 容量 91.8%', detail: '已用 91.8%，阈值 90%', firstSeenAt: now - 3 * HOUR, lastSeenAt: now - 5 * 60_000, resolvedAt: null, acknowledgedAt: null },
  { id: 'al-2', serverId: 'srv-1', serverName: 'web-01', kind: 'volume_full', severity: 'warning', title: '/data 容量 82.4%', detail: '已用 82.4%，阈值 80%', firstSeenAt: now - 2 * HOUR, lastSeenAt: now - 5 * 60_000, resolvedAt: null, acknowledgedAt: null },
  { id: 'al-3', serverId: 'srv-5', serverName: 'backup-01', kind: 'readonly_mount', severity: 'warning', title: '/backup 为只读挂载', detail: '该文件系统当前以只读方式挂载', firstSeenAt: now - 1 * HOUR, lastSeenAt: now - 5 * 60_000, resolvedAt: null, acknowledgedAt: null },
  { id: 'al-4', serverId: 'srv-3', serverName: 'db-01', kind: 'unreachable', severity: 'critical', title: '服务器不可达', detail: '最近一次巡检失败: 连接超时', firstSeenAt: now - 2 * HOUR, lastSeenAt: now - 5 * 60_000, resolvedAt: null, acknowledgedAt: null },
]

let knownHosts: KnownHost[] = [
  { id: 1, host: '10.0.1.11', port: 22, keyType: 'ssh-ed25519', fingerprint: 'SHA256:demoWeb01KeyFingerprint1234567890abc', createdAt: now - 20 * 24 * HOUR },
]

let settings: Settings = {
  theme: 'dark', concurrency: 6, connectTimeoutSecs: 8, probeTimeoutSecs: 20,
  cleanupConcurrency: 2, cleanupExpiryMins: 5, snapshotRetentionDays: 30,
  taskRetentionDays: 90, thresholdWarn: 80, thresholdCritical: 90, inodeWarn: 85,
  inodeCritical: 95, autoRefreshSecs: 60, quarantineDays: 7, allowPermanentDelete: false,
}

const usageScans: UsageScan[] = []
const usageRoots: Record<string, UsageNode> = {}
const usageChildren: Record<number, UsageNode[]> = {}
const largeFiles: Record<string, LargeFile[]> = {}
let nextNodeId = 100

function makeChildren(parent: UsageNode, rows: [string, number][]): UsageNode[] {
  return rows.map(([name, size], idx) => ({
    id: nextNodeId++, scanId: parent.scanId, parentId: parent.id, name,
    path: `${parent.path === '/' ? '' : parent.path}/${name}`, kind: 'dir', sizeBytes: size,
    depth: parent.depth + 1, childCount: 0, expandedAt: null,
  }))
}

function seedUsage(serverId: string, mount: string): UsageScan {
  const scan: UsageScan = {
    id: `scan-${serverId}-${mount.replace(/\//g, '_')}`, serverId, mount, rootPath: mount,
    status: 'succeeded', startedAt: now - 60_000, finishedAt: now - 55_000,
    totalBytes: 92 * GB, entries: 6, skipped: 0, partial: false, error: null,
  }
  const root: UsageNode = {
    id: nextNodeId++, scanId: scan.id, parentId: null, name: '/', path: mount, kind: 'dir',
    sizeBytes: scan.totalBytes, depth: 0, childCount: 6, expandedAt: now - 55_000,
  }
  usageRoots[scan.id] = root
  usageChildren[root.id] = makeChildren(root, [
    ['var', 42 * GB], ['home', 21 * GB], ['usr', 12 * GB], ['opt', 8 * GB], ['tmp', 5 * GB], ['etc', 4 * GB],
  ])
  for (const child of usageChildren[root.id]) {
    if (child.name === 'var') {
      usageChildren[child.id] = makeChildren(child, [
        ['lib', 18 * GB], ['log', 11 * GB], ['cache', 8 * GB], ['tmp', 5 * GB],
      ])
    }
    if (child.name === 'home') {
      usageChildren[child.id] = makeChildren(child, [
        ['deploy', 14 * GB], ['ubuntu', 7 * GB],
      ])
    }
  }
  largeFiles[scan.id] = [
    { id: 1, scanId: scan.id, path: `${mount === '/' ? '' : mount}/var/log/app.log`, sizeBytes: 4.2 * GB, mtime: now - 2 * HOUR },
    { id: 2, scanId: scan.id, path: `${mount === '/' ? '' : mount}/var/lib/docker/overlay2/cache.tar`, sizeBytes: 3.1 * GB, mtime: now - 26 * HOUR },
    { id: 3, scanId: scan.id, path: `${mount === '/' ? '' : mount}/home/deploy/archive.tar.gz`, sizeBytes: 1.8 * GB, mtime: now - 5 * 24 * HOUR },
  ]
  usageScans.push(scan)
  return scan
}

const recommendationMap: Record<string, ReclaimCandidate[]> = {
  'srv-1': [
    { serverId: 'srv-1', serverName: 'web-01', templateId: 'apt_cache', title: 'APT 下载缓存', description: '删除已下载的软件包缓存，可重新下载', sizeBytes: 2.4 * GB, risk: 'low', reversible: false, commandPreview: 'sudo -n apt-get clean', available: true },
    { serverId: 'srv-1', serverName: 'web-01', templateId: 'journal_7d', title: 'systemd journal', description: '保留最近 7 天日志，释放旧 journal', sizeBytes: 3.8 * GB, risk: 'medium', reversible: false, commandPreview: 'sudo -n journalctl --vacuum-time=7d', available: true },
    { serverId: 'srv-1', serverName: 'web-01', templateId: 'docker_dangling', title: 'Docker 悬空镜像', description: '清理悬空镜像与构建缓存，不处理 volume', sizeBytes: 6.2 * GB, risk: 'medium', reversible: false, commandPreview: 'sudo -n docker image prune -f', available: true },
    { serverId: 'srv-1', serverName: 'web-01', templateId: 'npm_cache', title: 'npm 缓存', description: '清理当前 SSH 用户的 npm 缓存', sizeBytes: 1.1 * GB, risk: 'low', reversible: false, commandPreview: 'npm cache clean --force', available: true },
  ],
  'srv-2': [
    { serverId: 'srv-2', serverName: 'web-02', templateId: 'apt_cache', title: 'APT 下载缓存', description: '删除已下载的软件包缓存', sizeBytes: 1.2 * GB, risk: 'low', reversible: false, commandPreview: 'sudo -n apt-get clean', available: true },
    { serverId: 'srv-2', serverName: 'web-02', templateId: 'journal_7d', title: 'systemd journal', description: '保留最近 7 天日志', sizeBytes: 1.7 * GB, risk: 'medium', reversible: false, commandPreview: 'sudo -n journalctl --vacuum-time=7d', available: true },
  ],
  'srv-4': [
    { serverId: 'srv-4', serverName: 'build-01', templateId: 'docker_dangling', title: 'Docker 悬空镜像', description: '清理悬空镜像与构建缓存', sizeBytes: 9.4 * GB, risk: 'medium', reversible: false, commandPreview: 'sudo -n docker image prune -f', available: true },
    { serverId: 'srv-4', serverName: 'build-01', templateId: 'tmp_old', title: '/tmp 旧文件', description: '删除 /tmp 中超过 7 天的普通文件', sizeBytes: 0.8 * GB, risk: 'high', reversible: false, commandPreview: 'sudo -n find /tmp -xdev -type f -mtime +7 -delete', available: true },
  ],
}

let quarantine: QuarantineEntry[] = [
  { id: 1, serverId: 'srv-1', serverName: 'web-01', originalPath: '/var/log/old-app.log', quarantinePath: '/.nova-rack-quarantine/demo/001-old-app.log', sizeBytes: 2.1 * GB, createdAt: now - 2 * HOUR, expiresAt: now + 6 * 24 * HOUR, status: 'stored' },
]

let cleanupPlans: Record<string, CleanupPlan> = {}

function delay<T>(value: T, ms = 120): Promise<T> {
  return new Promise(resolve => setTimeout(() => resolve(value), ms))
}

function tickTasks() {
  for (const task of tasks) {
    if (task.status !== 'running' && task.status !== 'queued') continue
    if (Date.now() - task.startedAt < 900) continue
    const hosts = taskHosts[task.id] ?? []
    const failed = hosts.filter(h => h.status === 'failed').length
    const partial = hosts.filter(h => h.status === 'partial').length
    task.succeeded = hosts.filter(h => h.status === 'succeeded').length
    task.failed = failed
    task.partial = partial
    task.status = failed > 0 || partial > 0 ? 'partial' : hosts.length ? 'succeeded' : 'failed'
    task.finishedAt = Date.now()
  }
}

function makeTask(title: string, ids: string[], kind = 'probe'): Task {
  const task: Task = {
    id: `task-${Date.now()}-${Math.floor(Math.random() * 1000)}`, kind, title,
    status: 'running', totalHosts: ids.length, succeeded: 0, failed: 0, partial: 0, cancelled: 0,
    concurrency: 4, startedAt: Date.now(), finishedAt: null, planId: null, error: null,
  }
  taskHosts[task.id] = ids.map((id, index) => ({
    id: index + 1, serverId: id, serverName: servers.find(s => s.id === id)?.name ?? id,
    status: 'queued', startedAt: null, finishedAt: null, durationMs: null, bytes: 0,
    errorCode: null, error: null, commandPreview: null, outputExcerpt: null, result: null,
  }))
  tasks.unshift(task)
  setTimeout(() => {
    const hosts = taskHosts[task.id] ?? []
    for (const host of hosts) {
      host.status = servers.find(s => s.id === host.serverId)?.lastStatus === 'offline' ? 'failed' : 'succeeded'
      host.startedAt = Date.now() - 600
      host.finishedAt = Date.now()
      host.durationMs = 600
      host.error = host.status === 'failed' ? '连接超时' : null
      host.errorCode = host.status === 'failed' ? 'CONNECT_TIMEOUT' : null
    }
    tickTasks()
  }, 1000)
  return task
}

export const demoApi: RackApi = {
  async getAppInfo() {
    return {
      version: '0.1.0', product: '服务器硬盘管理工具', dataDir: demoDataDir,
      knownHostsPath: demoKnownHostsPath,
      sshPath: demoSshPath, sshVersion: 'OpenSSH (demo)',
    } satisfies AppInfo
  },
  async diagnoseSsh() {
    return {
      sshPath: demoSshPath, sshVersion: 'OpenSSH (demo)',
      agentSocket: demoAgentSocket,
      agentKeys: ['256 SHA256:demoAgentKey (ED25519)'], agentError: null,
      knownHostsPath: demoKnownHostsPath,
      dataDir: demoDataDir, appVersion: '0.1.0', os: demoIsWindows ? 'windows' : 'macos',
    } satisfies SshDiagnostics
  },
  async listServers() { return delay(servers.map(s => ({ ...s, summary: { ...s.summary } }))) },
  async getServer(serverId) { return servers.find(s => s.id === serverId) ?? servers[0] },
  async createServer(input: ServerInput) {
    const server = summary({
      id: `srv-${servers.length + 1}`, name: input.name, alias: input.alias, host: input.host,
      port: input.port, user: input.user, authMode: input.authMode, keyPath: input.keyPath,
      groupName: input.groupName, tags: input.tags, lastStatus: 'online', lastSeenAt: Date.now(),
    }, 12, 2, 0, 0)
    servers.push(server)
    snapshotMap[server.id] = { ...snapshotMap['srv-1'], id: `snap-${server.id}`, serverId: server.id, collectedAt: Date.now() }
    return server
  },
  async updateServer(input: ServerUpdate) {
    const server = servers.find(s => s.id === input.id)
    if (!server) throw new Error('服务器不存在')
    Object.assign(server, {
      name: input.name, alias: input.alias, host: input.host, port: input.port, user: input.user,
      authMode: input.authMode, keyPath: input.keyPath, groupName: input.groupName, tags: input.tags,
    })
    return server
  },
  async deleteServer(serverId) {
    const index = servers.findIndex(s => s.id === serverId)
    if (index >= 0) servers.splice(index, 1)
  },
  async importSshConfig() {
    return delay<ImportCandidate[]>([
      { alias: 'prod-web', host: '10.0.1.11', user: 'deploy', port: 22, identityFile: '~/.ssh/id_ed25519', alreadyAdded: false },
      { alias: 'prod-db', host: '10.0.2.21', user: 'dba', port: 22, identityFile: '~/.ssh/id_ed25519', alreadyAdded: false },
      { alias: 'bastion', host: '10.0.9.9', user: 'ops', port: 2202, identityFile: null, alreadyAdded: true },
    ])
  },
  async testConnection(serverId) {
    const server = servers.find(s => s.id === serverId)
    return delay<ConnectionTest>({ status: 'online', message: '连接成功', latencyMs: 62, hostKey: null, serverId })
  },
  async trustHostKey() { return knownHosts },
  async listKnownHosts() { return delay(knownHosts) },
  async removeKnownHost(id) { knownHosts = knownHosts.filter(k => k.id !== id) },
  async getLatestSnapshot(serverId) { return snapshotMap[serverId] ?? null },
  async listSnapshots(serverId) { return snapshotMap[serverId] ? [snapshotMap[serverId]] : [] },
  async startBatchProbe(serverIds) { return makeTask(`批量巡检 ${serverIds.length} 台服务器`, serverIds) },
  async getTask(taskId) {
    tickTasks()
    const task = tasks.find(t => t.id === taskId)
    if (!task) throw new Error('任务不存在')
    return { task: { ...task }, hosts: (taskHosts[taskId] ?? []).map(h => ({ ...h })) }
  },
  async listTasks() { tickTasks(); return delay(tasks.map(t => ({ ...t }))) },
  async cancelTask(taskId) {
    const task = tasks.find(t => t.id === taskId)
    if (task) { task.status = 'cancelled'; task.cancelled = task.totalHosts; task.finishedAt = Date.now() }
  },
  async startUsageScan(serverId, mount) {
    const existing = usageScans.find(s => s.serverId === serverId && s.mount === mount)
    if (existing) return existing
    return delay(seedUsage(serverId, mount), 500)
  },
  async getUsageScan(scanId) { return usageScans.find(s => s.id === scanId) ?? seedUsage('srv-1', '/data') },
  async latestUsageScan(serverId) { return usageScans.find(s => s.serverId === serverId) ?? null },
  async cancelUsageScan() {},
  async usageRoot(scanId) { return usageRoots[scanId] ?? null },
  async listUsageChildren(_scanId, parentId) { return usageChildren[parentId] ?? [] },
  async expandUsageNode(nodeId) { return delay(usageChildren[nodeId] ?? []) },
  async scanLargeFiles(scanId) { return delay(largeFiles[scanId] ?? []) },
  async listLargeFiles(scanId) { return largeFiles[scanId] ?? [] },
  async getRecommendations(serverId) { return delay(recommendationMap[serverId] ?? []) },
  async sampleMetrics() {
    return delay<MetricSample>({
      collectedAt: Date.now(), cpuPercent: 23.4 + Math.random() * 6,
      memoryUsedBytes: 7.8 * GB, memoryTotalBytes: 16 * GB, load1: 0.42,
      disks: [
        { name: 'vda', readBytesPerSec: 1.2 * 1024 * 1024, writeBytesPerSec: 5.6 * 1024 * 1024, readIops: 32, writeIops: 148 },
        { name: 'vdb', readBytesPerSec: 0.4 * 1024 * 1024, writeBytesPerSec: 2.1 * 1024 * 1024, readIops: 8, writeIops: 54 },
      ],
    }, 300)
  },
  async listAlerts(activeOnly = true) { return alerts.filter(a => !activeOnly || !a.resolvedAt) },
  async acknowledgeAlert(alertId) { const a = alerts.find(x => x.id === alertId); if (a) a.acknowledgedAt = Date.now() },
  async resolveAlert(alertId) { const a = alerts.find(x => x.id === alertId); if (a) a.resolvedAt = Date.now() },
  async prepareCleanupPlan(request: CleanupRequest) {
    const items: CleanupPlan['items'] = []
    let id = 1
    for (const serverId of request.serverIds) {
      const server = servers.find(s => s.id === serverId)
      if (!server) continue
      for (const templateId of request.templateIds) {
        const candidate = (recommendationMap[serverId] ?? []).find(c => c.templateId === templateId)
        if (!candidate) continue
        items.push({
          id: id++, planId: 'plan-demo', serverId, serverName: server.name, kind: 'template',
          templateId, path: null, displayPath: `${server.name} · ${candidate.title}`, quarantinePath: null,
          sizeBytes: candidate.sizeBytes, risk: candidate.risk, reversible: false,
          commandPreview: candidate.commandPreview, defaultSelected: false, selected: true, status: 'planned', error: null,
        })
      }
    }
    for (const file of request.files) {
      const scan = usageScans.find(s => s.id === file.scanId)
      const server = servers.find(s => s.id === scan?.serverId) ?? servers[0]
      const lf = (largeFiles[file.scanId] ?? []).find(f => f.id === file.largeFileId)
      if (!lf) continue
      items.push({
        id: id++, planId: 'plan-demo', serverId: server.id, serverName: server.name, kind: file.action === 'delete' ? 'delete_file' : 'quarantine',
        templateId: null, path: lf.path, displayPath: lf.path, quarantinePath: file.action === 'delete' ? null : `/.nova-rack-quarantine/demo/${id}-${lf.path.split('/').pop()}`,
        sizeBytes: lf.sizeBytes, risk: file.action === 'delete' ? 'high' : 'medium', reversible: file.action !== 'delete',
        commandPreview: file.action === 'delete' ? `rm -f -- ${lf.path}` : `mv -- ${lf.path} <quarantine>`,
        defaultSelected: false, selected: true, status: 'planned', error: null,
      })
    }
    if (!items.length) throw new Error('没有可执行的清理项')
    const plan: CleanupPlan = {
      id: `plan-${Date.now()}`, token: `token-${Date.now()}`, status: 'planned',
      createdAt: Date.now(), expiresAt: Date.now() + 5 * 60_000, hostCount: new Set(items.map(i => i.serverId)).size,
      itemCount: items.length, totalBytes: items.reduce((sum, item) => sum + item.sizeBytes, 0),
      irreversible: items.filter(i => !i.reversible).length, items,
    }
    cleanupPlans[plan.id] = plan
    return delay(plan, 400)
  },
  async executeCleanupPlan(planId, token) {
    const plan = cleanupPlans[planId]
    if (!plan || plan.token !== token) throw new Error('清理令牌无效')
    if (plan.expiresAt < Date.now()) throw new Error('清理计划已过期')
    plan.status = 'completed'
    const task = makeTask(`空间回收 · ${plan.hostCount} 台服务器`, [...new Set(plan.items.map(i => i.serverId))], 'cleanup')
    task.planId = planId
    task.succeeded = task.totalHosts
    task.status = 'succeeded'
    task.finishedAt = Date.now()
    for (const host of taskHosts[task.id]) host.status = 'succeeded'
    for (const item of plan.items) item.status = 'succeeded'
    const newQuarantine: QuarantineEntry[] = plan.items.filter(i => i.kind === 'quarantine').map((item, index) => ({
      id: 100 + index, serverId: item.serverId, serverName: item.serverName, originalPath: item.path ?? '',
      quarantinePath: item.quarantinePath ?? '', sizeBytes: item.sizeBytes, createdAt: Date.now(),
      expiresAt: Date.now() + 7 * 86_400_000, status: 'stored',
    }))
    quarantine = [...newQuarantine, ...quarantine]
    return delay<CleanupExecution>({ taskId: task.id, planId, status: 'running' }, 300)
  },
  async getCleanupPlan(planId) { return cleanupPlans[planId] },
  async listQuarantine(serverId) { return quarantine.filter(q => !serverId || q.serverId === serverId) },
  async restoreQuarantine(entryId) {
    const entry = quarantine.find(q => q.id === entryId)
    if (entry) entry.status = 'restored'
    return { ok: true, message: '已恢复到原路径', taskId: null }
  },
  async purgeQuarantine(entryId) {
    const entry = quarantine.find(q => q.id === entryId)
    if (entry) entry.status = 'purged'
    return { ok: true, message: '隔离项已永久释放', taskId: null }
  },
  async getSettings() { return { ...settings } },
  async saveSettings(next) { settings = { ...next }; return settings },
  async exportReport(path) { return path },
  async listAudit() { return [] },
  async frontendReady() { return 'ok' },
}

export function demoReclaimCandidates(serverId: string) {
  return recommendationMap[serverId] ?? []
}
