<script setup lang="ts">
import { ref, watch } from 'vue'
import type { HostKeyChallenge, KnownHost } from '../types'
import { api } from '../services/api'
import { reportError } from '../services/ui'
import Icon from './Icon.vue'

const props = defineProps<{ open: boolean; challenge: HostKeyChallenge | null; changed?: boolean }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'trusted', hosts: KnownHost[]): void }>()
const trusting = ref(false)

watch(() => props.open, open => { if (open) trusting.value = false })

function isChanged() {
  return props.changed || !props.challenge?.pendingId
}

async function trust() {
  if (!props.challenge?.pendingId || trusting.value) return
  trusting.value = true
  try {
    const hosts = await api.trustHostKey(props.challenge.pendingId)
    emit('trusted', hosts)
    emit('close')
  } catch (error) {
    reportError(error)
  } finally {
    trusting.value = false
  }
}
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal hostkey-modal" role="dialog" aria-modal="true" aria-labelledby="hostkey-title">
      <header class="modal-head">
        <div><span class="eyebrow">HOST KEY VERIFICATION</span><h2 id="hostkey-title">{{ isChanged() ? '主机密钥已变化' : '确认主机指纹' }}</h2></div>
        <button class="icon-button" aria-label="关闭" @click="emit('close')"><Icon name="x" :size="18" /></button>
      </header>
      <div class="modal-body">
        <div v-if="isChanged()" class="notice danger"><Icon name="warning" :size="18" /><p>该主机的主机密钥与已信任记录不一致。为避免中间人攻击，连接已阻断。请先在“设置 → 主机密钥”中删除旧记录，并确认服务器确实更换过密钥，再重新连接。</p></div>
        <div v-else class="notice"><Icon name="shield" :size="18" /><p>首次连接 {{ challenge?.host }}:{{ challenge?.port }}。请通过带外方式（控制台、服务商面板）核对指纹后再信任。</p></div>
        <div v-if="challenge" class="fingerprint-list">
          <div v-for="fp in challenge.fingerprints" :key="fp.fingerprint" class="fingerprint-row">
            <span class="tag">{{ fp.keyType }}</span>
            <code>{{ fp.fingerprint }}</code>
          </div>
        </div>
        <p class="form-hint">信任后写入应用私有 known_hosts，不会自动接受后续变化。指纹不匹配时请取消并检查网络中间人风险。</p>
      </div>
      <footer class="modal-foot">
        <button class="button ghost" @click="emit('close')">{{ isChanged() ? '关闭' : '取消' }}</button>
        <button v-if="!isChanged()" class="button primary" :disabled="trusting" @click="trust"><Icon name="shield" :size="16" />{{ trusting ? '正在写入…' : '核对无误，信任' }}</button>
      </footer>
    </div>
  </div>
</template>
