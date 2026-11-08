<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZInput } from '@codex-proxy/ui/input'
import { ZSelect } from '@codex-proxy/ui/select'
import { ZCheckbox } from '@codex-proxy/ui/checkbox'
import { ZSwitch } from '@codex-proxy/ui/switch'
import {
  ArchiveRestore, BellRing, CalendarClock, Check, ChevronDown, CircleAlert, Clock, Copy, DatabaseBackup, Download, Fingerprint,
  Info, RefreshCw, RotateCcw, Save, ShieldAlert, SlidersHorizontal, TriangleAlert, Undo2, Upload, UserX,
} from '@lucide/vue'
import { api, date } from '../api'
import type { Account, Banks, Config, ErrorTrigger, Group } from '../types'

const props = defineProps<{ config: Config; revision: number; banks: Banks; keys: { id: string; name: string }[]; accounts: Account[]; groups: Group[]; selected: string[]; nextDue: number | null; scheduleError: string | null; busy: boolean; errorTrigger?: ErrorTrigger }>()
const emit = defineEmits<{ save: [config: Config]; bank: [operation: 'check' | 'apply' | 'rollback' | 'builtin']; error: [message: string] }>()

const uid = useId()
const id = (name: string) => `${uid}-${name}`

function normalizeScheduleFilter(config: Config) {
  const filter = config.schedule.filter
  if (!Array.isArray(filter.plans)) filter.plans = []
  // Older saved configs used a single plan value; load it into the multi-select.
  if (!filter.plans.length && filter.plan) {
    filter.plans = [filter.plan]
    filter.plan = ''
  }
}
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value))
const aliasText = (config: Config) => Object.entries(config.aliases).map(([a, b]) => `${a}=${b}`).join('\n')
const parseCodes = (text: string) => text.split(/[,，\s]+/).filter(Boolean).map(Number)
const uniqueCodes = (codes: number[]) => [...new Set(codes.map(Number).filter(Number.isInteger))]
function parseAliases(text: string): Record<string, string> | null {
  const mapping: Record<string, string> = {}
  for (const line of text.split('\n').filter(l => l.trim())) {
    const i = line.indexOf('=')
    if (i < 1 || !line.slice(i + 1).trim()) return null
    mapping[line.slice(0, i).trim()] = line.slice(i + 1).trim()
  }
  return mapping
}

const cfg = ref<Config>(clone(props.config))
normalizeScheduleFilter(cfg.value)
const aliases = ref(aliasText(cfg.value))
const codesText = ref(cfg.value.triggerErrorCodes.join(', '))
const models = ref<string[]>([])
const loadingModels = ref(false)
const configFile = ref<HTMLInputElement>()
const importing = ref(false)
const fileNotice = ref('')
const rulesNotice = ref('')
const exportedJson = ref('')
const exportedField = ref<HTMLTextAreaElement>()
const showModels = ref(false)
watch(codesText, text => { cfg.value.triggerErrorCodes = parseCodes(text) }, { flush: 'sync' })

// Dirty tracking: compare what save() would send with the saved config,
// using sorted keys so key order from imports does not matter.
const stable = (value: unknown) => JSON.stringify(value, (_key, v) => v && typeof v === 'object' && !Array.isArray(v) ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : v)
function snapshot(config: Config, aliasMap: Record<string, string>) {
  const c = clone(config)
  normalizeScheduleFilter(c)
  c.aliases = aliasMap
  c.triggerErrorCodes = uniqueCodes(c.triggerErrorCodes)
  if (c.schedule.filter.plans.length) c.schedule.filter.plan = ''
  // Order of these lists carries no meaning; ignore it for change detection.
  c.degradedModels = [...c.degradedModels].sort()
  c.schedule.filter.plans = [...c.schedule.filter.plans].sort()
  return stable(c)
}
const savedSnapshot = computed(() => snapshot(props.config, props.config.aliases))
const dirty = computed(() => {
  const mapping = parseAliases(aliases.value)
  return !mapping || snapshot(cfg.value, mapping) !== savedSnapshot.value
})

type NumberKey = 'maxOutputTokens' | 'timeoutSeconds' | 'maxConcurrency' | 'lowConfidencePercent' | 'degradedPercent' | 'confirmationRounds' | 'lowConfidenceRetries' | 'failureRetries' | 'retryDelaySeconds' | 'retentionPerAccount' | 'triggerCooldownMinutes' | 'updateIntervalHours'
interface NumberField { key: NumberKey; label: string; min: number; max?: number; step?: number; help: string; detail?: string }
const number = (event: Event, key: NumberKey) => { cfg.value[key] = Number((event.target as HTMLInputElement).value) }
const requestNumbers: NumberField[] = [
  { key: 'maxOutputTokens', label: '输出 token 上限', min: 512, max: 16384, help: '范围 512–16384。' },
  { key: 'timeoutSeconds', label: '单次请求超时（秒）', min: 0, step: 1, help: '默认 120 秒，0 表示不设超时；后台总预算 180 秒。',
    detail: '执行时会提前预留校验和保存时间；插件后台总预算为 180 秒，默认 120 秒请求可完整等待，更长请求仍受本次剩余预算约束。预算内收到的部分正文通过样本校验后可接纳；仅推理内容不能作为样本。' },
  { key: 'maxConcurrency', label: '并行检测数（批内同时探测账户数）', min: 1, max: 16, step: 1, help: '默认 4，最多 16；过大可能触发计量 Key 限流。',
    detail: '同一后台回调内最多并行检测的账户数，可提升批量检测整体速度；宿主限制单次回调最多 16 路并发流，过大也可能触发计量 Key 限流而浪费重试。' },
]
const ruleNumbers: NumberField[] = [
  { key: 'lowConfidencePercent', label: '低置信度复测阈值（%）', min: 0, max: 100, step: 0.1, help: '最高候选概率低于此值时自动复测。' },
  { key: 'degradedPercent', label: '降智判定阈值（%）', min: 0, max: 100, step: 0.1, help: '命中降级名单且概率达到此值即判定降智。' },
  { key: 'confirmationRounds', label: '连续确认轮数', min: 1, max: 5, help: '连续判定降智达到该轮数才确认（1–5）。' },
  { key: 'lowConfidenceRetries', label: '低置信度额外复测次数', min: 0, max: 5, help: '低置信度时最多追加的复测次数（0–5）。' },
  { key: 'failureRetries', label: '失败后重试（次）', min: 0, max: 99, step: 1, help: '每轮采样失败后额外重试 0–99 次，默认 4；0 表示只请求一次。认证、权限、参数拒绝也计入。' },
  { key: 'retentionPerAccount', label: '每账号保留检测记录数', min: 1, max: 100, help: '每个账户只保留最近 N 条检测记录（1–100），超出的旧记录在提交检测、保存参数和后台维护时清理。' },
  { key: 'retryDelaySeconds', label: '复测与失败重试间隔（秒）', min: 0, max: 3600, help: '两次复测或失败重试之间的等待（0–3600）。' },
]

const keyOptions = computed(() => props.keys.map(k => ({ value: k.id, label: k.name })))
const modelOptions = computed(() => models.value.map(m => ({ value: m, label: m })))
const effortOptions = ['', 'none', 'minimal', 'low', 'medium', 'high', 'xhigh'].map(s => ({ value: s, label: s || '不传递' }))
const targetOptions = computed(() => props.groups.map(g => ({ value: g.id, label: g.name })))
const groupOptions = computed(() => [{ value: '', label: '全部分组' }, { value: '__ungrouped__', label: '未分组' }, ...props.groups.map(g => ({ value: g.id, label: g.name }))])
const planOptions = computed(() => Array.from(new Set(props.accounts.map(a => a.planType || 'unknown'))).sort().map(p => ({ value: p, label: p })))
const statusOptions = [{ value: '', label: '全部已启用状态' }, ...['normal', 'error', 'quota_exhausted', 'rate_limited'].map(value => ({ value, label: ({ normal: '正常', error: '异常', quota_exhausted: '额度耗尽', rate_limited: '限流' } as Record<string, string>)[value]! }))]
const kindOptions = [{ value: 'interval', label: '固定间隔' }, { value: 'daily', label: '每日时刻' }]
const scopeOptions = [{ value: 'filter', label: '动态匹配筛选条件' }, { value: 'ids', label: '固定账户列表' }]
const rejectedOptions = [{ value: 'continue', label: '保留账户调度，下次继续自动检测' }, { value: 'disable', label: '停止该账户调度' }]

// Degraded-model chips grouped by family; selections missing from the
// active bank stay visible so they can be removed before saving.
const familyNames: Record<string, string> = { gpt: 'GPT', claude: 'Claude', gemini: 'Gemini', grok: 'Grok', deepseek: 'DeepSeek', qwen: 'Qwen' }
const modelFamilies = computed(() => {
  const families = new Map<string, { value: string; label: string }[]>()
  for (const m of props.banks.active.models) {
    const family = m.family || 'other'
    if (!families.has(family)) families.set(family, [])
    families.get(family)!.push({ value: m.id, label: m.label })
  }
  const known = new Set(props.banks.active.models.map(m => m.id))
  const missing = cfg.value.degradedModels.filter(m => !known.has(m))
  return [
    ...[...families].map(([family, list]) => ({ family, name: familyNames[family] || family, models: list, missing: false })),
    ...(missing.length ? [{ family: '__missing__', name: '不在当前库', models: missing.map(m => ({ value: m, label: m })), missing: true }] : []),
  ]
})
const toggleModel = (model: string, checked: boolean) => {
  cfg.value.degradedModels = checked ? [...cfg.value.degradedModels, model] : cfg.value.degradedModels.filter(x => x !== model)
}

const invalidCodes = computed(() => codesText.value.split(/[,，\s]+/).filter(Boolean).filter(t => !/^\d+$/.test(t) || Number(t) < 100 || Number(t) > 599))
const triggerState = computed(() => {
  const t = props.errorTrigger
  if (!t) return null
  return t.enabled ? { tone: 'tone-success', text: `监听中 · ${t.codes.join(', ')}` } : { tone: 'tone-neutral', text: '未开启' }
})

const bankMeta = computed(() => [
  { label: '当前提交', value: props.banks.active.commit, mono: true },
  { label: 'SHA-256', value: props.banks.active.hash, mono: true },
  { label: '建库时间', value: date(props.banks.active.builtAt), mono: false },
  { label: '候选模型', value: `${props.banks.active.models.length} 个`, mono: false },
  { label: '最近检查', value: date(props.banks.update.checkedAt), mono: false },
  { label: '评分算法提交', value: props.banks.engineCommit, mono: true },
])

function useDefaults() {
  Object.assign(cfg.value, { lowConfidencePercent: 20, degradedPercent: 90, confirmationRounds: 1, lowConfidenceRetries: 1, failureRetries: 4, requestRejectedAction: 'continue', retryDelaySeconds: 10, retentionDays: 3, retentionPerAccount: 10, triggerErrorCodes: [], triggerCooldownMinutes: 30 })
  codesText.value = ''
  rulesNotice.value = '已填入默认规则（失败重试 4 次、请求拒绝后保留调度，并清空日志触发错误码、冷却恢复 30 分钟），保存后生效。'
}
function exportConfig() {
  const data = { format: 'modeltrace-health-config', version: 1, pluginId: 'personal.modeltrace-health', config: props.config }
  exportedJson.value = JSON.stringify(data, null, 2) + '\n'
  // RS embeds this page with sandbox="allow-scripts", which blocks downloads.
  // Always expose the JSON so backup also works inside that isolated frame.
  if (window.top === window) {
    const url = URL.createObjectURL(new Blob([exportedJson.value], { type: 'application/json' }))
    const link = document.createElement('a'); link.href = url; link.download = 'modeltrace-health-config.json'; link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  }
  fileNotice.value = '已导出当前已保存的参数；可复制下方 JSON 并保存为 modeltrace-health-config.json。'
}
async function copyConfig() {
  try {
    if (navigator.clipboard) { await navigator.clipboard.writeText(exportedJson.value) }
    else { exportedField.value?.select(); if (!document.execCommand('copy')) throw new Error('copy failed') }
    fileNotice.value = '配置已复制，请保存为 modeltrace-health-config.json。'
  } catch {
    exportedField.value?.select()
    fileNotice.value = '请复制已选中的 JSON，并保存为 modeltrace-health-config.json。'
  }
}
async function importConfig(event: Event) {
  const input = event.target as HTMLInputElement, file = input.files?.[0]
  if (!file) return
  importing.value = true; fileNotice.value = ''
  try {
    if (file.size > 250000) throw new Error('配置文件不能超过250KB')
    const data = JSON.parse(await file.text())
    if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('配置文件须为JSON对象')
    const wrapped = Object.hasOwn(data, 'format')
    if (wrapped && (data.format !== 'modeltrace-health-config' || data.version !== 1 || data.pluginId !== 'personal.modeltrace-health')) throw new Error('配置文件类型或版本不受支持')
    const result = await api.previewConfig(wrapped ? data.config : data)
    cfg.value = result.config
    normalizeScheduleFilter(cfg.value)
    aliases.value = aliasText(cfg.value)
    codesText.value = cfg.value.triggerErrorCodes.join(', ')
    fileNotice.value = '配置已载入表单，请核对计量 Key、目标分组和定时范围，再保存。'
  } catch (e) { emit('error', (e as Error).message) }
  finally { importing.value = false; input.value = '' }
}
async function fetchModels() {
  loadingModels.value = true
  try { models.value = (await api.models(cfg.value.keyId)).models } catch (e) { emit('error', (e as Error).message) } finally { loadingModels.value = false }
}
function save() {
  const mapping = parseAliases(aliases.value)
  if (!mapping) { emit('error', '模型别名每行须为 请求名称=指纹库模型ID'); return }
  cfg.value.aliases = mapping
  cfg.value.triggerErrorCodes = uniqueCodes(cfg.value.triggerErrorCodes)
  if (cfg.value.schedule.filter.plans.length) cfg.value.schedule.filter.plan = ''
  emit('save', clone(cfg.value))
}
</script>

<template>
  <form class="settings" @submit.prevent="save">
    <header class="savebar" :class="{ floating: dirty }">
      <div class="savebar-text">
        <h2>检测参数</h2>
        <p>手动与定时检测共用；保存后用于新任务，运行中的任务沿用原配置。</p>
      </div>
      <div class="savebar-actions">
        <span aria-live="polite"><span v-if="dirty" class="pill dot tone-warning">有未保存的修改</span><span v-else class="saved"><Check :size="14" />已保存</span></span>
        <ZButton type="primary" :loading="busy" :disabled="importing" @click="save"><template #icon><Save :size="15" /></template>保存全部参数</ZButton>
      </div>
    </header>

    <!-- 1. Request -->
    <section class="panel" :aria-labelledby="id('request')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><SlidersHorizontal :size="17" /></span><div><h3 :id="id('request')">检测请求</h3><p>发送探测请求所用的 Key、模型与生成参数。</p></div></div>
      </div>
      <div class="grid">
        <div class="field"><span class="field-label">计量 Key</span><ZSelect v-model="cfg.keyId" :options="keyOptions" placeholder="选择已有 Key" aria-label="计量Key" /></div>
        <div class="field"><label class="field-label" :for="id('model')">请求模型</label><ZInput :id="id('model')" v-model="cfg.requestModel" placeholder="gpt-6.1-sol" aria-label="请求模型" /></div>
        <div class="field">
          <span class="field-label">从模型目录选择</span>
          <div class="inline-control"><ZSelect v-model="cfg.requestModel" :options="modelOptions" placeholder="先读取，或直接手动输入" aria-label="模型目录" /><ZButton :disabled="!cfg.keyId" :loading="loadingModels" @click="fetchModels"><template #icon><RefreshCw :size="14" /></template>读取</ZButton></div>
        </div>
        <div class="field">
          <label class="field-label" :for="id('temperature')">temperature</label>
          <input :id="id('temperature')" class="control" :value="cfg.temperature ?? ''" type="number" min="0" max="2" step="0.1" placeholder="模型默认" :aria-describedby="id('temperature-help')" @input="cfg.temperature = ($event.target as HTMLInputElement).value === '' ? null : Number(($event.target as HTMLInputElement).value)">
          <span :id="id('temperature-help')" class="help">0–2，留空使用模型默认值。</span>
        </div>
        <div class="field"><span class="field-label">推理强度</span><ZSelect v-model="cfg.reasoningEffort" :options="effortOptions" aria-label="推理强度" /></div>
        <div v-for="f in requestNumbers" :key="f.key" class="field">
          <label class="field-label" :for="id(f.key)">{{ f.label }}</label>
          <input :id="id(f.key)" class="control" type="number" :min="f.min" :max="f.max" :step="f.step" :value="cfg[f.key]" :aria-describedby="id(`${f.key}-help`)" @input="number($event, f.key)">
          <span :id="id(`${f.key}-help`)" class="help" :class="{ detail: f.detail }" :title="f.detail">{{ f.help }}<Info v-if="f.detail" :size="12" class="help-icon" /></span>
        </div>
      </div>
    </section>

    <!-- 2. Verdict rules -->
    <section class="panel" :aria-labelledby="id('rules')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><ShieldAlert :size="17" /></span><div><h3 :id="id('rules')">降智判定与复测</h3><p>请求模型不在降级名单内，而最高候选命中名单且概率达到阈值时，按确认轮数判定降智。</p></div></div>
        <div class="panel-actions"><ZButton :disabled="busy" title="阈值 20% / 90%，确认 1 轮，复测 1 次，失败重试 4 次，保留账户调度，间隔 10 秒，并清空日志触发错误码" @click="useDefaults"><template #icon><RotateCcw :size="14" /></template>使用默认规则</ZButton></div>
      </div>
      <div v-if="rulesNotice" class="notice tone-info block" role="status"><Info :size="15" /><span>{{ rulesNotice }}</span></div>

      <div class="models" role="group" :aria-labelledby="id('models')">
        <div class="models-head">
          <span :id="id('models')" class="field-label">降级模型名单</span>
          <span class="pill" :class="cfg.degradedModels.length ? 'tone-primary' : 'tone-danger'">已选 {{ cfg.degradedModels.length }} / {{ banks.active.models.length }}</span>
          <span class="help">点选即可加入或移出名单，至少选择 1 个。</span>
        </div>
        <div v-for="fam in modelFamilies" :key="fam.family" class="family">
          <span class="family-name" :class="{ 'text-warning': fam.missing }">{{ fam.name }}</span>
          <div class="chips">
            <label v-for="m in fam.models" :key="m.value" class="chip" :class="{ on: cfg.degradedModels.includes(m.value), missing: fam.missing }" :title="fam.missing ? '当前指纹库不存在此模型，保存前请移出' : m.value">
              <input type="checkbox" class="visually-hidden" :checked="cfg.degradedModels.includes(m.value)" @change="toggleModel(m.value, ($event.target as HTMLInputElement).checked)">
              <span class="tick"><Check v-if="cfg.degradedModels.includes(m.value)" :size="10" :stroke-width="3.5" /></span>{{ m.label }}
            </label>
          </div>
        </div>
      </div>

      <div class="grid">
        <div v-for="f in ruleNumbers" :key="f.key" class="field">
          <label class="field-label" :for="id(f.key)">{{ f.label }}</label>
          <input :id="id(f.key)" class="control" type="number" :min="f.min" :max="f.max" :step="f.step" :value="cfg[f.key]" :aria-describedby="id(`${f.key}-help`)" @input="number($event, f.key)">
          <span :id="id(`${f.key}-help`)" class="help">{{ f.help }}</span>
        </div>
        <div class="field span-all">
          <span class="field-label">请求被拒绝后的处理</span>
          <ZSelect v-model="cfg.requestRejectedAction" :options="rejectedOptions" aria-label="请求被拒绝后的处理" />
          <span class="help">认证、Key 权限或模型参数被拒绝且失败重试耗尽后生效。保留调度时，已启用的定时或日志触发可在下次继续检测；停止调度后需在宿主手动恢复。此项独立于确认降智后的处理。</span>
        </div>
        <div class="field span-all">
          <label class="field-label" :for="id('aliases')">模型别名</label>
          <textarea :id="id('aliases')" v-model="aliases" class="control mono" rows="3" placeholder="my-luna=gpt-5.6-luna" spellcheck="false" :aria-describedby="id('aliases-help')" />
          <span :id="id('aliases-help')" class="help">每行一条：请求名称=指纹库模型ID，用于自定义请求模型名映射到指纹库模型。</span>
        </div>
      </div>
    </section>

    <!-- 3. Actions after confirmation -->
    <section class="panel" :aria-labelledby="id('actions')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><UserX :size="17" /></span><div><h3 :id="id('actions')">确认降智后的处理</h3><p>达到确认轮数后自动执行；两项可同时开启，分别执行。</p></div></div>
      </div>
      <div class="options">
        <div class="option" :class="{ on: cfg.autoDisable }">
          <div class="option-main">
            <ZCheckbox v-model="cfg.autoDisable" label="确认降智后自动关闭该账户调度" />
            <span class="help option-help">账户停止接收调度，需由管理员在宿主中手动恢复。</span>
          </div>
        </div>
        <div class="option" :class="{ on: cfg.autoMoveGroup }">
          <div class="option-main">
            <ZCheckbox v-model="cfg.autoMoveGroup" label="确认降智后移入指定分组" />
            <span class="help option-help">把账户移入目标分组，便于集中隔离与复查。</span>
          </div>
          <div v-if="cfg.autoMoveGroup" class="option-target">
            <span class="field-label">目标分组</span>
            <ZSelect v-model="cfg.targetGroupId" :options="targetOptions" placeholder="选择已有分组" aria-label="降智目标分组" />
          </div>
        </div>
      </div>
      <div class="notice tone-warning block"><TriangleAlert :size="15" /><span>移组会用目标分组<b>替换</b>账户原有的全部分组，而不是追加。</span></div>
    </section>

    <!-- 4. Request log trigger -->
    <section class="panel" :aria-labelledby="id('trigger')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><BellRing :size="17" /></span><div><h3 :id="id('trigger')">请求日志触发</h3><p>读取 CPR 请求错误日志，上游或客户端状态码（含诊断信息中的 status=503）命中后，立即为对应账户排队检测；插件自身的检测请求不会触发；同一账户在冷却时间内只触发一次。留空关闭。</p></div></div>
        <div v-if="triggerState" class="panel-actions"><span class="pill dot" :class="triggerState.tone">{{ triggerState.text }}</span></div>
      </div>
      <div class="grid">
        <div class="field">
          <label class="field-label" :for="id('codes')">触发错误码</label>
          <input :id="id('codes')" v-model="codesText" class="control" placeholder="例如 503, 429" :aria-describedby="id('codes-help')">
          <span v-if="invalidCodes.length" :id="id('codes-help')" class="help text-danger">无效错误码：{{ invalidCodes.join('、') }}（须为 100–599 的整数）</span>
          <span v-else :id="id('codes-help')" class="help">逗号或空格分隔，100–599，最多 32 个；留空关闭。</span>
        </div>
        <div class="field">
          <label class="field-label" :for="id('cooldown')">冷却时间（分钟）</label>
          <input :id="id('cooldown')" class="control" type="number" min="0" max="1440" step="1" :value="cfg.triggerCooldownMinutes" :aria-describedby="id('cooldown-help')" @input="number($event, 'triggerCooldownMinutes')">
          <span :id="id('cooldown-help')" class="help">默认 30，范围 0–1440；0 表示每次命中都触发。</span>
        </div>
        <div v-if="errorTrigger" class="field">
          <span class="field-label">运行状态</span>
          <dl class="stats">
            <div><dt>最近读取</dt><dd class="tabular">{{ date(errorTrigger.checkedAt) }}</dd></div>
            <div><dt>最近命中</dt><dd v-if="errorTrigger.matchedAt" :title="`${errorTrigger.matchedAccount || '未知账户'} · ${errorTrigger.matchedCode} · ${date(errorTrigger.matchedAt)}`">{{ errorTrigger.matchedAccount || '未知账户' }} · <b>{{ errorTrigger.matchedCode }}</b> · <span class="tabular">{{ date(errorTrigger.matchedAt) }}</span></dd><dd v-else class="muted">暂无</dd></div>
            <div><dt>累计触发</dt><dd class="tabular">{{ errorTrigger.matchedTotal }} 次</dd></div>
          </dl>
        </div>
      </div>
      <div v-if="errorTrigger?.error" class="notice tone-danger block"><CircleAlert :size="15" /><span>最近错误：{{ errorTrigger.error }}</span></div>
    </section>

    <!-- 5. Schedule -->
    <section class="panel" :aria-labelledby="id('schedule')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><CalendarClock :size="17" /></span><div><h3 :id="id('schedule')">自动定时检测</h3><p>按计划自动检测匹配范围内已启用的 OpenAI 账户。</p></div></div>
        <div class="panel-actions"><ZSwitch v-model="cfg.schedule.enabled" label="开启定时检测" /></div>
      </div>
      <div class="grid">
        <div class="field"><span class="field-label">定时方式</span><ZSelect v-model="cfg.schedule.kind" :options="kindOptions" aria-label="定时方式" /></div>
        <div v-if="cfg.schedule.kind === 'interval'" class="field"><label class="field-label" :for="id('interval')">间隔（分钟）</label><input :id="id('interval')" v-model.number="cfg.schedule.intervalMinutes" class="control" type="number" min="1" max="43200"></div>
        <div v-else class="field"><label class="field-label" :for="id('time')">每日时间</label><input :id="id('time')" v-model="cfg.schedule.time" class="control" type="time" required></div>
        <div class="field"><span class="field-label">时区（IANA）</span><ZInput v-model="cfg.schedule.timezone" placeholder="Asia/Shanghai" aria-label="时区" /></div>
      </div>
      <div class="subhead">检测范围</div>
      <div class="grid">
        <div class="field"><span class="field-label">账户范围</span><ZSelect v-model="cfg.schedule.scope" :options="scopeOptions" aria-label="定时账户范围" /></div>
        <template v-if="cfg.schedule.scope === 'filter'">
          <div class="field"><span class="field-label">订阅类型</span><ZSelect v-model="cfg.schedule.filter.plans" multiple clearable collapse-tags :options="planOptions" placeholder="全部订阅" aria-label="定时订阅筛选" /></div>
          <div class="field"><span class="field-label">账户状态</span><ZSelect v-model="cfg.schedule.filter.status" :options="statusOptions" aria-label="定时状态筛选" /></div>
          <div class="field"><span class="field-label">分组</span><ZSelect v-model="cfg.schedule.filter.group" :options="groupOptions" aria-label="定时分组筛选" /></div>
          <div class="field"><span class="field-label">账户关键词</span><ZInput v-model="cfg.schedule.filter.keyword" placeholder="名称、邮箱或ID" aria-label="定时关键词" /></div>
        </template>
        <div v-else class="field span-all">
          <div class="field-row">
            <label class="field-label" :for="id('ids')">固定账户列表</label>
            <span class="pill tone-neutral">已保存 {{ cfg.schedule.accountIds.length }} 个账户</span>
            <ZButton size="small" :disabled="!selected.length" @click="cfg.schedule.accountIds = [...selected]">使用账户页所选（{{ selected.length }}）</ZButton>
          </div>
          <textarea :id="id('ids')" class="control mono" :value="cfg.schedule.accountIds.join('\n')" rows="3" placeholder="每行一个账户ID" spellcheck="false" @input="cfg.schedule.accountIds = ($event.target as HTMLTextAreaElement).value.split(/\s+/).filter(Boolean)" />
        </div>
      </div>
      <div class="meta-line"><Clock :size="14" /><span>下次执行：<b class="tabular">{{ date(nextDue) }}</b></span></div>
      <div v-if="scheduleError" class="notice tone-danger block"><CircleAlert :size="15" /><span>最近定时提交失败：{{ scheduleError }}</span></div>
    </section>

    <!-- 6. Fingerprint bank -->
    <section class="panel" :aria-labelledby="id('bank')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><Fingerprint :size="17" /></span><div><h3 :id="id('bank')">ModelTrace 指纹库</h3><p>来源 <span class="mono source">https://github.com/xqy2006/ModelTrace</span>；下载经兼容性与离线评分校验后应用，进行中的检测继续使用原版本。</p></div></div>
        <div class="panel-actions"><ZButton :loading="busy" @click="emit('bank', 'check')"><template #icon><RefreshCw :size="14" /></template>检查更新</ZButton></div>
      </div>
      <dl class="meta-grid">
        <div v-for="item in bankMeta" :key="item.label" class="meta-item"><dt>{{ item.label }}</dt><dd :class="{ mono: item.mono, tabular: !item.mono }" :title="item.value">{{ item.value }}</dd></div>
      </dl>
      <div v-if="banks.update.error" class="notice tone-danger block" role="alert"><CircleAlert :size="15" /><span>{{ banks.update.error }}</span></div>
      <div v-if="banks.update.available" class="update">
        <div class="update-text">
          <b>发现新指纹库 · <span class="mono">{{ banks.update.available.commit.slice(0, 12) }}</span></b>
          <span>新增：{{ banks.update.added.join('、') || '无' }}</span>
          <span>移除：{{ banks.update.removed.join('、') || '无' }}</span>
        </div>
        <ZButton type="primary" :loading="busy" @click="emit('bank', 'apply')"><template #icon><Download :size="14" /></template>下载并应用</ZButton>
      </div>
      <div class="bank-row">
        <div class="button-row">
          <ZButton :disabled="!banks.previous || busy" @click="emit('bank', 'rollback')"><template #icon><Undo2 :size="14" /></template>回退上一版本</ZButton>
          <ZButton :disabled="banks.active.hash === banks.builtin.hash || busy" @click="emit('bank', 'builtin')"><template #icon><ArchiveRestore :size="14" /></template>恢复内置库</ZButton>
        </div>
        <button type="button" class="link-toggle" :aria-expanded="showModels" @click="showModels = !showModels">{{ showModels ? '收起模型列表' : `查看当前全部模型（${banks.active.models.length}）` }}<ChevronDown :size="14" :class="{ flipped: showModels }" /></button>
      </div>
      <div v-if="showModels" class="chips model-list"><span v-for="m in banks.active.models" :key="m.id" class="chip static" :title="m.id">{{ m.label }}</span></div>
      <div class="subhead">自动更新</div>
      <div class="grid">
        <div class="field">
          <span class="field-label">自动检查更新</span>
          <div class="switch-cell"><ZSwitch v-model="cfg.autoCheckUpdates" label="后台定期检查（手动应用）" /></div>
        </div>
        <div class="field"><label class="field-label" :for="id('updateIntervalHours')">检查间隔（小时）</label><input :id="id('updateIntervalHours')" class="control" type="number" min="1" max="720" :value="cfg.updateIntervalHours" @input="number($event, 'updateIntervalHours')"></div>
        <div class="field">
          <span class="field-label">HTTPS 镜像前缀（可选）</span>
          <ZInput v-model="cfg.mirrorPrefix" placeholder="https://your-mirror.example" aria-label="GitHub镜像前缀" :aria-describedby="id('mirror-help')" />
          <span :id="id('mirror-help')" class="help">实际地址为 镜像前缀/完整 GitHub 地址，保存后生效。</span>
        </div>
      </div>
    </section>

    <!-- 7. Backup -->
    <section class="panel" :aria-labelledby="id('backup')">
      <div class="panel-head">
        <div class="panel-title"><span class="panel-icon"><DatabaseBackup :size="17" /></span><div><h3 :id="id('backup')">配置备份</h3><p>导出已保存参数（不含 API 密钥或账号凭据），重装或迁移后导入；导入后需核对并保存才生效。</p></div></div>
        <div class="panel-actions">
          <input ref="configFile" type="file" accept=".json,application/json" aria-label="配置文件" hidden @change="importConfig">
          <ZButton :disabled="busy || importing" @click="exportConfig"><template #icon><Download :size="14" /></template>导出配置</ZButton>
          <ZButton :disabled="busy" :loading="importing" @click="configFile?.click()"><template #icon><Upload :size="14" /></template>导入配置</ZButton>
        </div>
      </div>
      <div v-if="fileNotice" class="notice tone-info block" role="status"><Info :size="15" /><span>{{ fileNotice }}</span></div>
      <div v-if="exportedJson" class="export">
        <div class="button-row">
          <ZButton :disabled="busy" @click="copyConfig"><template #icon><Copy :size="14" /></template>复制配置 JSON</ZButton>
          <ZButton variant="text" @click="exportedJson = ''">收起备份</ZButton>
        </div>
        <textarea ref="exportedField" class="control mono json" :value="exportedJson" readonly rows="8" aria-label="导出的配置JSON" spellcheck="false" />
      </div>
      <span v-else-if="!fileNotice" class="help">参数保存在宿主中，更新同一插件实例会自动沿用。</span>
    </section>
  </form>
</template>

<style scoped>
.settings { display: block; min-width: 0; }

/* Save bar: sits at the top and follows the scroll while there are unsaved edits. */
.savebar {
  position: relative; z-index: 20;
  display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px 16px;
  margin-bottom: 16px; padding: 12px 16px 12px 20px;
  background: var(--z-card-bg, #fff);
  border: 1px solid var(--z-color-border, #d8e0ea);
  border-radius: var(--z-border-radius-lg, 12px);
  box-shadow: var(--z-box-shadow-tertiary, none);
}
.savebar.floating { position: sticky; top: 0; box-shadow: var(--z-box-shadow, 0 8px 24px -12px rgba(0,0,0,.25)); }
.savebar-text { flex: 1 1 260px; min-width: 0; }
.savebar h2 { font-size: 16px; font-weight: 600; line-height: 1.4; color: var(--z-color-text, #0e1726); }
.savebar p { margin-top: 2px; font-size: 12px; line-height: 1.5; }
.savebar-actions { display: flex; align-items: center; gap: 12px; margin-left: auto; }
.saved { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--z-color-text-tertiary, #535b67); }

/* Grid of fields */
.grid { display: grid; grid-template-columns: minmax(0, 1fr); gap: 18px 20px; }
.span-all { grid-column: 1 / -1; }
.field-row { display: flex; align-items: center; flex-wrap: wrap; gap: 8px 12px; }
.inline-control { display: flex; gap: 8px; min-width: 0; }
.inline-control > :first-child { flex: 1; min-width: 0; }
.help-icon { display: inline-block; margin-left: 4px; vertical-align: -2px; }
.help.detail { cursor: help; }
.subhead {
  margin: 22px 0 14px; padding-top: 18px;
  border-top: 1px solid var(--z-color-border-secondary, var(--z-color-border, #e2e8f0));
  font-size: 13px; font-weight: 600; color: var(--z-color-text, #0e1726);
}
.notice.block { margin-top: 16px; }
.notice.block:first-child, .panel-head + .notice.block { margin-top: 0; margin-bottom: 16px; }
.button-row { display: flex; flex-wrap: wrap; gap: 8px; }

/* Native controls matching the host input look */
.control {
  width: 100%; min-width: 0; height: var(--z-control-height, 38px); padding: 0 14px;
  border: 0; border-radius: var(--z-border-radius, 8px); outline: 0;
  background: var(--z-input-bg, #587ec113); color: var(--z-color-text, #0e1726);
  box-shadow: var(--z-input-shadow, none);
  font-size: var(--z-font-size, 13px); font-weight: 600;
  transition: background-color .16s, box-shadow .16s;
}
.control:hover:not(:focus) { background: var(--z-input-hover-bg, #fff); box-shadow: var(--z-input-hover-shadow, none); }
.control:focus { background: var(--z-input-active-bg, #fff); box-shadow: var(--z-input-active-shadow, none); }
.control::placeholder { font-weight: 400; color: var(--z-color-text-quaternary, #5f6876); }
.control[type=number] { appearance: textfield; -moz-appearance: textfield; }
.control[type=number]::-webkit-inner-spin-button, .control[type=number]::-webkit-outer-spin-button { margin: 0; appearance: none; -webkit-appearance: none; }
textarea.control { height: auto; padding: 10px 14px; line-height: 1.6; resize: vertical; font-weight: 500; }
textarea.control.mono { font-size: 12px; }
html[data-theme='dark'] .control { color-scheme: dark; }
.static-value {
  display: flex; align-items: center; gap: 8px; height: var(--z-control-height, 38px); padding: 0 14px;
  border-radius: var(--z-border-radius, 8px); background: var(--z-color-fill-quaternary, #f6f8fb);
  font-weight: 600; color: var(--z-color-text, #0e1726);
}
.static-value svg { color: var(--z-color-text-tertiary, #535b67); }
.switch-cell { display: flex; align-items: center; min-height: var(--z-control-height, 38px); }

/* Degraded model chips */
.models {
  margin-bottom: 20px; padding: 14px 16px 16px;
  border-radius: var(--z-border-radius, 8px); background: var(--z-color-fill-quaternary, #f6f8fb);
  display: grid; gap: 12px;
}
.models-head { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 10px; }
.models-head .help { flex: 1 1 200px; }
.family { display: grid; grid-template-columns: 76px minmax(0, 1fr); gap: 6px 12px; align-items: start; }
.family-name { font-size: 12px; font-weight: 600; line-height: 30px; color: var(--z-color-text-tertiary, #535b67); }
.chips { display: flex; flex-wrap: wrap; gap: 8px; }
.chip {
  display: inline-flex; align-items: center; gap: 7px; height: 30px; padding: 0 12px 0 9px;
  border: 1px solid var(--z-color-border, #d8e0ea); border-radius: 999px;
  background: var(--z-card-bg, #fff); color: var(--z-color-text-secondary, #414b5a);
  font-size: 12px; font-weight: 500; line-height: 1; white-space: nowrap;
  cursor: pointer; user-select: none; transition: background-color .15s, border-color .15s, color .15s;
}
.chip:hover { border-color: var(--z-color-primary-border, #8493b9); }
.chip.on { background: var(--z-color-primary-container, #f0f4fe); border-color: var(--z-color-primary-border, #8493b9); color: var(--z-color-primary-on-container, #4261b5); font-weight: 600; }
.chip.missing { border-style: dashed; border-color: var(--z-color-warning-border, #a89169); color: var(--z-color-warning-text, #955900); background: var(--z-color-warning-container, #fef7eb); }
.chip:has(input:focus-visible) { outline: 2px solid var(--z-control-outline, #8493b9); outline-offset: 2px; }
.chip.static { padding: 0 11px; cursor: default; }
.chip.static:hover { border-color: var(--z-color-border, #d8e0ea); }
.tick {
  display: grid; place-items: center; width: 14px; height: 14px; flex: none; border-radius: 50%;
  box-shadow: inset 0 0 0 1.5px var(--z-color-border, #d8e0ea); color: var(--z-button-primary-color, #fff);
}
.chip.on .tick { background: var(--z-color-primary, #5983f4); box-shadow: none; }
.chip.missing .tick { box-shadow: inset 0 0 0 1.5px currentColor; }
.chip.missing.on .tick { background: currentColor; }
.chip.missing.on .tick svg { color: var(--z-color-warning-container, #fef7eb); }
.visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }

/* Action options: one row per action */
.options { display: grid; overflow: hidden; border: 1px solid var(--z-color-border, #d8e0ea); border-radius: var(--z-border-radius, 8px); }
.option {
  display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px 24px;
  min-width: 0; padding: 14px 16px; transition: background-color .15s;
}
.option + .option { border-top: 1px solid var(--z-color-border, #d8e0ea); }
.option.on { background: var(--z-color-fill-quaternary, #f6f8fb); }
.option-main { display: flex; flex-direction: column; gap: 6px; min-width: 0; flex: 1 1 280px; }
.option-help { padding-left: 26px; }
.option-target { display: flex; align-items: center; gap: 10px; flex: 0 1 340px; min-width: 0; }
.option-target > :last-child { flex: 1; min-width: 0; }
.option-target .field-label { white-space: nowrap; font-size: 13px; }

/* Trigger status */
.stats { display: grid; gap: 6px; padding: 10px 14px; border-radius: var(--z-border-radius, 8px); background: var(--z-color-fill-quaternary, #f6f8fb); font-size: 12px; }
.stats > div { display: grid; grid-template-columns: 64px minmax(0, 1fr); gap: 10px; align-items: baseline; }
.stats dt { color: var(--z-color-text-tertiary, #535b67); }
.stats dd { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--z-color-text, #0e1726); }

/* Schedule */
.meta-line { display: flex; align-items: center; gap: 8px; margin-top: 18px; font-size: 12px; color: var(--z-color-text-secondary, #414b5a); }
.meta-line b { font-weight: 600; color: var(--z-color-text, #0e1726); }
.meta-line svg { color: var(--z-color-text-tertiary, #535b67); flex: none; }

/* Fingerprint bank */
.source { user-select: all; overflow-wrap: anywhere; }
.meta-grid {
  display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1px; overflow: hidden;
  border: 1px solid var(--z-color-border, #d8e0ea); border-radius: var(--z-border-radius, 8px);
  background: var(--z-color-border, #d8e0ea);
}
.meta-item { display: flex; flex-direction: column; gap: 3px; min-width: 0; padding: 10px 14px; background: var(--z-card-bg, #fff); }
.meta-item dt { font-size: 12px; color: var(--z-color-text-tertiary, #535b67); }
.meta-item dd { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; color: var(--z-color-text, #0e1726); }
.meta-item dd.mono { font-size: 12px; user-select: all; }
.update {
  display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 12px 16px;
  margin-top: 16px; padding: 12px 16px; border-radius: var(--z-border-radius, 8px);
  background: var(--z-color-info-container, #f2f5fe); color: var(--z-color-info-on-container, #4261b5);
}
.update-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; }
.update-text b { font-size: 13px; font-weight: 600; }
.bank-row { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 10px 20px; margin-top: 16px; }
.link-toggle {
  display: inline-flex; align-items: center; gap: 4px; padding: 4px 0; border: 0; background: none; cursor: pointer;
  font-size: 12px; color: var(--z-color-primary-text, #4d71d2);
}
.link-toggle:hover { color: var(--z-color-primary-text-hover, #5c75b0); }
.link-toggle:focus-visible { outline: 2px solid var(--z-control-outline, #8493b9); outline-offset: 2px; border-radius: 4px; }
.link-toggle svg { transition: transform .15s; }
.link-toggle .flipped { transform: rotate(180deg); }
.model-list { margin-top: 12px; padding: 12px; border-radius: var(--z-border-radius, 8px); background: var(--z-color-fill-quaternary, #f6f8fb); }

/* Backup */
.export { display: grid; gap: 10px; margin-top: 16px; }
.json { min-height: 160px; }

@media (min-width: 600px) {
  .grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
@media (min-width: 950px) {
  .grid, .meta-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}
@media (max-width: 599px) {
  .savebar { padding: 10px 12px 10px 14px; border-radius: var(--z-border-radius, 8px); }
  .savebar p { display: none; }
  .savebar-text { flex-basis: auto; }
  .savebar-actions { gap: 8px; }
  .family { grid-template-columns: minmax(0, 1fr); }
  .family-name { line-height: 1.5; }
  .models { padding: 12px; }
  .option-target { flex-basis: 100%; flex-direction: column; align-items: stretch; gap: 6px; padding-left: 26px; }
}
</style>
