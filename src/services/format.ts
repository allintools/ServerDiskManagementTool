export function formatBytes(value: number | null | undefined, digits = 1): string {
  if (value == null || !Number.isFinite(value)) return '—'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB']
  let n = Math.max(0, value)
  let i = 0
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++ }
  return `${n.toFixed(i === 0 ? 0 : digits)} ${units[i]}`
}

export function formatPercent(value: number | null | undefined, digits = 1): string {
  if (value == null || !Number.isFinite(value)) return '—'
  return `${Math.min(100, Math.max(0, value)).toFixed(digits)}%`
}

export function formatNumber(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '—'
  return new Intl.NumberFormat('zh-CN').format(value)
}

export function formatTime(value: number | null | undefined): string {
  if (!value) return '—'
  return new Date(value).toLocaleString('zh-CN', { hour12: false })
}

export function timeAgo(value: number | null | undefined): string {
  if (!value) return '从未'
  const diff = Date.now() - value
  if (diff < 0) return '刚刚'
  const sec = Math.floor(diff / 1000)
  if (sec < 60) return `${sec} 秒前`
  const min = Math.floor(sec / 60)
  if (min < 60) return `${min} 分钟前`
  const hour = Math.floor(min / 60)
  if (hour < 24) return `${hour} 小时前`
  return `${Math.floor(hour / 24)} 天前`
}

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return '—'
  if (ms < 1000) return `${Math.max(0, Math.round(ms))} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} 秒`
  return `${Math.floor(ms / 60_000)} 分 ${Math.round((ms % 60_000) / 1000)} 秒`
}

export function statusText(status: string): string {
  const map: Record<string, string> = {
    online: '在线', offline: '离线', degraded: '部分异常', unknown: '未巡检', error: '错误',
    host_key_unknown: '待确认指纹', host_key_changed: '密钥已变化', running: '运行中', queued: '排队中',
    succeeded: '成功', failed: '失败', partial: '部分完成', cancelled: '已取消', interrupted: '已中断',
    planned: '待执行', completed: '已完成', skipped: '已跳过', stored: '隔离中', restored: '已恢复',
    purged: '已清空',
  }
  return map[status] ?? status
}

export type Tone = 'ok' | 'warn' | 'danger' | 'muted' | 'info'
export function statusTone(status: string): Tone {
  if (['online', 'succeeded', 'completed', 'restored', 'purged'].includes(status)) return 'ok'
  if (['degraded', 'partial', 'warning', 'host_key_unknown', 'planned', 'stored'].includes(status)) return 'warn'
  if (['offline', 'error', 'failed', 'critical', 'host_key_changed'].includes(status)) return 'danger'
  if (['running', 'queued'].includes(status)) return 'info'
  return 'muted'
}

export function riskText(risk: string): string {
  return { low: '低风险', medium: '中风险', high: '高风险' }[risk] ?? risk
}

export function severityText(severity: string): string {
  return { info: '提示', warning: '警告', critical: '严重' }[severity] ?? severity
}

export function privilegeText(privilege: string | null | undefined): string {
  if (!privilege) return '未知'
  return { root: 'root', sudo: '免密 sudo', user: '普通用户' }[privilege] ?? privilege
}

export function shortPath(path: string, max = 56): string {
  if (!path || path.length <= max) return path || '—'
  const head = path.slice(0, Math.floor(max * 0.55))
  const tail = path.slice(-Math.floor(max * 0.35))
  return `${head}…${tail}`
}

export function basename(path: string): string {
  if (!path) return ''
  const trimmed = path.replace(/\/+$/, '')
  return trimmed.split('/').pop() || '/'
}

export function osLabel(os?: string | null, distro?: string | null): string {
  if (distro) return distro
  if (!os) return '未知系统'
  return os === 'Linux' ? 'Linux' : os
}
