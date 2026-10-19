import { reactive } from 'vue'
import type { ApiErrorShape } from '../types'

export const ui = reactive({
  toast: '',
  toastKind: 'ok' as 'ok' | 'error',
  busy: false,
  demo: false,
})

let toastTimer: ReturnType<typeof setTimeout> | undefined

export function notify(message: string, kind: 'ok' | 'error' = 'ok') {
  ui.toast = message
  ui.toastKind = kind
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => { ui.toast = '' }, 4200)
}

export function messageOf(error: unknown): string {
  if (!error) return '未知错误'
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  const shape = error as ApiErrorShape
  if (shape?.message) {
    return shape.code ? `${shape.message}` : shape.message
  }
  try { return JSON.stringify(error) } catch { return '未知错误' }
}

export function reportError(error: unknown) {
  const message = messageOf(error)
  console.error(error)
  notify(message, 'error')
}

export function setBusy(value: boolean) {
  ui.busy = value
}
