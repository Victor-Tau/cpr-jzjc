<script setup lang="ts">
import { computed, ref } from 'vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZInput } from '@codex-proxy/ui/input'
import { ZSelect } from '@codex-proxy/ui/select'
import { ZCheckbox } from '@codex-proxy/ui/checkbox'
import { api, date } from '../api'
import type { Account, Banks, Config, Group } from '../types'
const props = defineProps<{ config: Config; revision: number; banks: Banks; keys: { id: string; name: string }[]; accounts: Account[]; groups: Group[]; selected: string[]; nextDue: number | null; scheduleError: string | null; busy: boolean }>()
const emit = defineEmits<{ save: [config: Config]; bank: [operation: 'check' | 'apply' | 'rollback' | 'builtin']; error: [message: string] }>()
const cfg = ref<Config>(JSON.parse(JSON.stringify(props.config)))
function normalizeScheduleFilter(config: Config) {
  const filter = config.schedule.filter
  if (!Array.isArray(filter.plans)) filter.plans = []
  // Older saved configs used a single plan value; load it into the multi-select.
  if (!filter.plans.length && filter.plan) {
    filter.plans = [filter.plan]
    filter.plan = ''
  }
}
normalizeScheduleFilter(cfg.value)
const aliases = ref(Object.entries(cfg.value.aliases).map(([a,b]) => `${a}=${b}`).join('\n'))
const models = ref<string[]>([])
const configFile = ref<HTMLInputElement>()
const importing = ref(false)
const fileNotice = ref('')
const exportedJson = ref('')
const exportedField = ref<HTMLTextAreaElement>()
const targetOptions = computed(() => props.groups.map(g => ({ value:g.id, label:g.name })))
function useDefaults() {
  Object.assign(cfg.value, {lowConfidencePercent:20, degradedPercent:90, confirmationRounds:1, lowConfidenceRetries:1, retryDelaySeconds:10, retentionDays:3})
  fileNotice.value='已填入 1.4 默认规则，保存后生效。'
}
function exportConfig() {
  const data = { format:'modeltrace-health-config', version:1, pluginId:'personal.modeltrace-health', config:props.config }
  exportedJson.value=JSON.stringify(data,null,2)+'\n'
  // RS embeds this page with sandbox="allow-scripts", which blocks downloads.
  // Always expose the JSON so backup also works inside that isolated frame.
  if(window.top===window) {
    const url=URL.createObjectURL(new Blob([exportedJson.value],{type:'application/json'}))
    const link=document.createElement('a'); link.href=url; link.download='modeltrace-health-config.json'; link.click()
    setTimeout(()=>URL.revokeObjectURL(url),1000)
  }
  fileNotice.value='已导出当前已保存的参数；可复制下方 JSON 并保存为 modeltrace-health-config.json。'
}
async function copyConfig() {
  try {
    if(navigator.clipboard) { await navigator.clipboard.writeText(exportedJson.value) }
    else { exportedField.value?.select(); if(!document.execCommand('copy'))throw new Error('copy failed') }
    fileNotice.value='配置已复制，请保存为 modeltrace-health-config.json。'
  } catch {
    exportedField.value?.select()
    fileNotice.value='请复制已选中的 JSON，并保存为 modeltrace-health-config.json。'
  }
}
async function importConfig(event:Event) {
  const input=event.target as HTMLInputElement, file=input.files?.[0]
  if(!file)return
  importing.value=true; fileNotice.value=''
  try {
    if(file.size>250000)throw new Error('配置文件不能超过250KB')
    const data=JSON.parse(await file.text())
    if(!data || typeof data!=='object' || Array.isArray(data))throw new Error('配置文件须为JSON对象')
    const wrapped=Object.hasOwn(data,'format')
    if(wrapped && (data.format!=='modeltrace-health-config' || data.version!==1 || data.pluginId!=='personal.modeltrace-health'))throw new Error('配置文件类型或版本不受支持')
    const result=await api.previewConfig(wrapped ? data.config : data)
    cfg.value=result.config
    normalizeScheduleFilter(cfg.value)
    aliases.value=Object.entries(cfg.value.aliases).map(([a,b])=>`${a}=${b}`).join('\n')
    fileNotice.value='配置已载入表单，请核对计量 Key、目标分组和定时范围，再保存。'
  } catch(e) { emit('error',(e as Error).message) }
  finally { importing.value=false; input.value='' }
}
const loadingModels = ref(false)
const modelOptions = computed(() => models.value.map(m => ({ value: m, label: m })))
const degradedOptions = computed(() => props.banks.active.models.map(m => ({ value: m.id, label: m.label })))
const groupOptions = computed(() => [{ value: '', label: '全部分组' }, { value: '__ungrouped__', label: '未分组' }, ...props.groups.map(g => ({ value: g.id, label: g.name }))])
const planOptions = computed(() => Array.from(new Set(props.accounts.map(a => a.planType || 'unknown'))).sort().map(p => ({ value: p, label: p })))
const statusOptions = [{ value: '', label: '全部已启用状态' }, ...['normal','error','quota_exhausted','rate_limited'].map(value => ({ value, label: ({ normal:'正常',error:'异常',quota_exhausted:'额度耗尽',rate_limited:'限流' } as Record<string,string>)[value]! }))]
const number = (event: Event, key: keyof Config) => { (cfg.value as any)[key] = Number((event.target as HTMLInputElement).value) }
async function fetchModels() { loadingModels.value = true; try { models.value = (await api.models(cfg.value.keyId)).models } catch(e) { emit('error', (e as Error).message) } finally { loadingModels.value = false } }
function save() {
  const mapping: Record<string,string> = {}
  for (const line of aliases.value.split('\n').filter(l => l.trim())) {
    const i = line.indexOf('='); if (i < 1 || !line.slice(i+1).trim()) { emit('error','模型别名每行须为 请求名称=指纹库模型ID'); return }
    mapping[line.slice(0,i).trim()] = line.slice(i+1).trim()
  }
  cfg.value.aliases = mapping
  if (cfg.value.schedule.filter.plans.length) cfg.value.schedule.filter.plan = ''
  emit('save', JSON.parse(JSON.stringify(cfg.value)))
}
</script>

<template>
  <form @submit.prevent="save">
    <div class="section-heading"><div><h2>检测参数</h2><p>手动与定时检测共用。保存后用于新任务，已运行任务继续使用原配置。</p></div><ZButton type="primary" :loading="busy" :disabled="importing" @click="save">保存全部参数</ZButton></div>
    <section class="panel"><div class="section-heading"><div><h3>配置备份</h3><p>参数自动保存在宿主中，更新同一插件实例会沿用。重装或迁移前可导出 JSON 文件。</p></div><div class="inline"><ZButton :disabled="busy || importing" @click="exportConfig">导出配置</ZButton><ZButton :disabled="busy" :loading="importing" @click="configFile?.click()">导入配置</ZButton></div></div><input ref="configFile" type="file" accept=".json,application/json" aria-label="配置文件" hidden @change="importConfig"><p>导出已保存参数，不含 API 密钥或账号凭据；导入后核对并保存才会生效。</p><p v-if="fileNotice" role="status">{{ fileNotice }}</p><div v-if="exportedJson"><div class="inline"><ZButton :disabled="busy" @click="copyConfig">复制配置 JSON</ZButton><ZButton variant="text" @click="exportedJson=''">收起备份</ZButton></div><textarea ref="exportedField" :value="exportedJson" readonly rows="8" aria-label="导出的配置JSON" spellcheck="false" /></div></section>
    <section class="panel form-grid">
      <label>计量 Key<ZSelect v-model="cfg.keyId" :options="keys.map(k => ({ value:k.id,label:k.name }))" placeholder="选择已有 Key" aria-label="计量Key" /></label>
      <label>请求模型<ZInput v-model="cfg.requestModel" placeholder="gpt-6.1-sol" aria-label="请求模型" /></label>
      <div class="field"><span>从模型目录选择</span><div class="inline"><ZSelect v-model="cfg.requestModel" :options="modelOptions" placeholder="可手动输入请求模型" aria-label="模型目录" /><ZButton :disabled="!cfg.keyId" :loading="loadingModels" @click="fetchModels">读取</ZButton></div></div>
      <label>temperature（留空使用默认）<input :value="cfg.temperature ?? ''" type="number" min="0" max="2" step="0.1" @input="cfg.temperature = ($event.target as HTMLInputElement).value === '' ? null : Number(($event.target as HTMLInputElement).value)"></label>
      <label>推理强度<ZSelect v-model="cfg.reasoningEffort" :options="['','none','minimal','low','medium','high','xhigh'].map(s => ({ value:s,label:s || '不传递' }))" aria-label="推理强度" /></label>
      <label>输出 token 上限<input type="number" min="512" max="16384" :value="cfg.maxOutputTokens" @input="number($event,'maxOutputTokens')"></label>
      <label>单次请求超时（秒）<input type="number" min="0" step="1" :value="cfg.timeoutSeconds" @input="number($event,'timeoutSeconds')"><small>默认120秒，0表示不设请求超时。执行时会提前预留校验和保存时间；RS 3.30.2 的后台总预算为30秒，单任务请求最长约29秒，16路并行约26.5秒；预算内收到的部分正文通过样本校验后可接纳；仅推理内容不能作为样本。</small></label>
      <label>并行检测数（批内同时探测账户数）<input type="number" min="1" max="16" step="1" :value="cfg.maxConcurrency" @input="number($event,'maxConcurrency')"><small>默认4。同一后台回调内最多并行检测的账户数，提升批量检测整体速度；宿主限制单次回调最多16路并发流，过大也可能触发计量Key限流而浪费重试。</small></label>
    </section>

    <section class="panel"><div class="section-heading"><h3>降智与复测规则</h3><ZButton :disabled="busy" @click="useDefaults">使用 1.4 默认规则</ZButton></div><p>请求模型不在降级名单内，最高候选命中名单且概率达到阈值时，按配置轮数确认。</p>
      <div class="form-grid">
        <div class="field wide"><span>降级模型名单</span><div class="checks"><ZCheckbox v-for="m in degradedOptions" :key="m.value" :model-value="cfg.degradedModels.includes(m.value)" :label="m.label" @change="checked => cfg.degradedModels = checked ? [...cfg.degradedModels,m.value] : cfg.degradedModels.filter(x => x !== m.value)" /></div></div>
        <label>低置信度复测阈值（%）<input type="number" min="0" max="100" step="0.1" :value="cfg.lowConfidencePercent" @input="number($event,'lowConfidencePercent')"><small>最高候选概率低于此值时自动复测。</small></label>
        <label>降智判定阈值（%）<input type="number" min="0" max="100" step="0.1" :value="cfg.degradedPercent" @input="number($event,'degradedPercent')"></label>
        <label>连续确认轮数<input type="number" min="1" max="5" :value="cfg.confirmationRounds" @input="number($event,'confirmationRounds')"></label>
        <label>低置信度额外复测次数<input type="number" min="0" max="5" :value="cfg.lowConfidenceRetries" @input="number($event,'lowConfidenceRetries')"></label>
        <label>复测与失败重试间隔（秒）<input type="number" min="0" max="3600" :value="cfg.retryDelaySeconds" @input="number($event,'retryDelaySeconds')"></label>
        <div class="field"><span>历史保留</span><small>自动保存最近3天，过期记录在提交检测和后台维护时清理；不限制记录条数，仅受宿主存储预算约束。</small></div>
        <label class="wide">模型别名（每行：请求名称=指纹库模型ID）<textarea v-model="aliases" rows="3" placeholder="my-luna=gpt-5.6-luna" /></label>
      </div>
      <div class="callout"><div class="checks"><ZCheckbox v-model="cfg.autoDisable" label="确认降智后自动关闭该账户调度" /><ZCheckbox v-model="cfg.autoMoveGroup" label="确认降智后移入指定分组" /></div><label v-if="cfg.autoMoveGroup" class="action-target">目标分组<ZSelect v-model="cfg.targetGroupId" :options="targetOptions" placeholder="选择已有分组" aria-label="降智目标分组" /></label><p>两项可同时开启，达到确认轮数后分别执行。移组会将原有分组替换为目标分组；停用后由管理员在宿主中恢复。</p></div>
    </section>

    <section class="panel"><div class="section-heading"><h3>自动定时检测</h3><ZCheckbox v-model="cfg.schedule.enabled" label="开启定时检测" /></div>
      <div class="form-grid">
        <label>定时方式<ZSelect v-model="cfg.schedule.kind" :options="[{value:'interval',label:'固定间隔'},{value:'daily',label:'每日时刻'}]" aria-label="定时方式" /></label>
        <label v-if="cfg.schedule.kind === 'interval'">间隔（分钟）<input v-model.number="cfg.schedule.intervalMinutes" type="number" min="1" max="43200"></label>
        <label v-else>每日时间<input v-model="cfg.schedule.time" type="time" required></label>
        <label>时区（IANA）<ZInput v-model="cfg.schedule.timezone" placeholder="Asia/Shanghai" aria-label="时区" /></label>
        <label>账户范围<ZSelect v-model="cfg.schedule.scope" :options="[{value:'filter',label:'动态匹配筛选条件'},{value:'ids',label:'固定账户列表'}]" aria-label="定时账户范围" /></label>
        <template v-if="cfg.schedule.scope === 'filter'">
          <label>订阅类型<ZSelect v-model="cfg.schedule.filter.plans" multiple clearable collapse-tags :options="planOptions" placeholder="全部订阅" aria-label="定时订阅筛选" /></label>
          <label>账户状态<ZSelect v-model="cfg.schedule.filter.status" :options="statusOptions" aria-label="定时状态筛选" /></label>
          <label>分组<ZSelect v-model="cfg.schedule.filter.group" :options="groupOptions" aria-label="定时分组筛选" /></label>
          <label>账户关键词<ZInput v-model="cfg.schedule.filter.keyword" placeholder="名称、邮箱或ID" aria-label="定时关键词" /></label>
        </template>
        <div v-else class="field wide"><div class="inline"><span>已保存 {{ cfg.schedule.accountIds.length }} 个账户</span><ZButton :disabled="!selected.length" @click="cfg.schedule.accountIds = [...selected]">使用账户页所选（{{ selected.length }}）</ZButton></div><textarea :value="cfg.schedule.accountIds.join('\n')" rows="3" placeholder="每行一个账户ID" @input="cfg.schedule.accountIds = ($event.target as HTMLTextAreaElement).value.split(/\s+/).filter(Boolean)" /></div>
      </div><p>定时仅检测已启用的 OpenAI 账户。下次执行：{{ date(nextDue) }}</p><p v-if="scheduleError" class="danger">最近定时提交失败：{{ scheduleError }}</p>
    </section>

    <section class="panel"><div class="section-heading"><div><h3>ModelTrace 指纹库</h3><p>来源：<span class="source-url">https://github.com/xqy2006/ModelTrace</span></p></div><ZButton :loading="busy" @click="emit('bank','check')">检查更新</ZButton></div>
      <dl class="bank-meta"><dt>当前提交</dt><dd>{{ banks.active.commit }}</dd><dt>SHA-256</dt><dd>{{ banks.active.hash }}</dd><dt>建库时间</dt><dd>{{ date(banks.active.builtAt) }}</dd><dt>候选模型</dt><dd>{{ banks.active.models.length }} 个</dd><dt>最近检查</dt><dd>{{ date(banks.update.checkedAt) }}</dd><dt>评分算法提交</dt><dd>{{ banks.engineCommit }}</dd></dl>
      <details><summary>查看当前全部模型</summary><div class="model-list"><span v-for="m in banks.active.models" :key="m.id">{{ m.label }}</span></div></details>
      <div v-if="banks.update.error" class="message danger" role="alert">{{ banks.update.error }}</div>
      <div v-if="banks.update.available" class="callout"><b>发现新指纹库 · {{ banks.update.available.commit.slice(0,12) }}</b><p>新增：{{ banks.update.added.join('、') || '无' }}<br>移除：{{ banks.update.removed.join('、') || '无' }}</p><ZButton type="primary" :loading="busy" @click="emit('bank','apply')">下载并应用</ZButton></div>
      <div class="inline bank-actions"><ZButton :disabled="!banks.previous || busy" @click="emit('bank','rollback')">回退上一版本</ZButton><ZButton :disabled="banks.active.hash === banks.builtin.hash || busy" @click="emit('bank','builtin')">恢复内置库</ZButton></div>
      <p>下载通过兼容性与离线评分校验后应用。进行中的检测与复测继续使用原版本。</p>
      <div class="form-grid"><div class="field"><ZCheckbox v-model="cfg.autoCheckUpdates" label="后台自动检查更新（手动应用）" /></div><label>检查间隔（小时）<input type="number" min="1" max="720" :value="cfg.updateIntervalHours" @input="number($event,'updateIntervalHours')"></label><label class="wide">HTTPS 镜像前缀（可选）<ZInput v-model="cfg.mirrorPrefix" placeholder="https://your-mirror.example" aria-label="GitHub镜像前缀" /><small>保存后生效；实际地址为 镜像前缀/完整GitHub地址。</small></label></div>
    </section>
  </form>
</template>
