<script setup lang="ts">
import { computed } from 'vue'
import type { DetectionUsage } from '../types'

const props = defineProps<{ usage?: DetectionUsage; attemptCount?: number }>()
const amount = computed(() => {
  if (!props.attemptCount) return '—'
  const usage = props.usage
  const costs = usage?.costs.map(c => c.currency === 'USD' ? `$${c.estimatedAmount}` : `${c.currency} ${c.estimatedAmount}`).join(' + ')
  if (!usage?.complete) return costs ? `${costs}（待同步）` : '待同步'
  return costs || '0'
})
</script>

<template><span class="detection-cost" title="来自平台使用统计，合计本次检测的全部请求（含失败重试和复测）；待同步表示平台尚未提供完整费用。">费用 {{ amount }}</span></template>
