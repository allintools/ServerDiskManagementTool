import { ref } from 'vue'
import { api } from './api'
import type { HostKeyChallenge, Server, Task } from '../types'
import { notify, reportError } from './ui'

export function useConnectFlow(onTask?: (task: Task) => void) {
  const show = ref(false)
  const challenge = ref<HostKeyChallenge | null>(null)
  const changed = ref(false)
  const current = ref<Server | null>(null)
  const connectingName = ref('')
  const queue = ref<Server[]>([])
  let busy = false

  async function startProbe(server: Server) {
    const task = await api.startBatchProbe([server.id])
    onTask?.(task)
    notify(`已开始巡检 ${server.name}`)
  }

  async function drain() {
    if (busy || show.value || !queue.value.length) return
    busy = true
    try {
      while (queue.value.length && !show.value) {
        const server = queue.value.shift()!
        connectingName.value = server.name
        try {
          const test = await api.testConnection(server.id)
          if (test.status === 'host_key_unknown' && test.hostKey) {
            current.value = server
            challenge.value = test.hostKey
            changed.value = false
            show.value = true
            return
          }
          if (test.status === 'host_key_changed') {
            current.value = server
            challenge.value = test.hostKey ?? null
            changed.value = true
            show.value = true
            return
          }
          if (test.status === 'online') {
            await startProbe(server)
          } else {
            notify(test.message || `连接 ${server.name} 失败`, 'error')
          }
        } catch (error) {
          reportError(error)
        }
      }
    } finally {
      busy = false
      connectingName.value = ''
    }
  }

  function enqueue(servers: Server[]) {
    queue.value.push(...servers)
    void drain()
  }

  async function trusted() {
    show.value = false
    changed.value = false
    const server = current.value
    current.value = null
    challenge.value = null
    if (server) {
      try { await startProbe(server) } catch (error) { reportError(error) }
    }
    void drain()
  }

  function closeChallenge() {
    show.value = false
    changed.value = false
    current.value = null
    challenge.value = null
    void drain()
  }

  return { show, challenge, changed, connectingName, enqueue, trusted, closeChallenge }
}
