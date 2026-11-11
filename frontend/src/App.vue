<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Activity, CalendarClock, ChevronRight, CircleAlert, CircleCheck, Fingerprint, History, ListChecks, Play, RefreshCw, RotateCcw, ScrollText, Search, Settings2, ShieldAlert, Square, TriangleAlert, Users, X, Zap } from '@lucide/vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZInput } from '@codex-proxy/ui/input'
import { ZSelect } from '@codex-proxy/ui/select'
import { ZCheckbox } from '@codex-proxy/ui/checkbox'
import { ZTable, type TableColumn } from '@codex-proxy/ui/table'
import { ZPagination } from '@codex-proxy/ui/pagination'
import { ZDialog } from '@codex-proxy/ui/dialog'
import Settings from './components/Settings.vue'
import { api, active, date, percent, status, statusLabels, verdict } from './api'
import { createRequestId } from './request-id'
import type { Account, Banks, Config, ErrorTrigger, Group, Job, Report } from './types'

type Tone = 'success' | 'danger' | 'warning' | 'info' | 'primary' | 'neutral'
type Diagnostics = NonNullable<Report['attempts'][number]['diagnostics']>

const tab = ref('accounts'), loading = ref(true), busy = ref(false), error = ref(''), notice = ref('')
const tabs = [{ key:'accounts', label:'账户检测', icon:Users }, { key:'settings', label:'参数配置', icon:Settings2 }, { key:'history', label:'检测历史', icon:History }]
const accounts = ref<Account[]>([]), jobs = ref<Job[]>([]), config = ref<Config>(), banks = ref<Banks>()
const allGroups = ref<Group[]>([])
const keys = ref<{ id:string; name:string }[]>([]), revision = ref(0), settingsKey = ref(0), nextDue = ref<number|null>(null), scheduleError = ref<string|null>(null)
const errorTrigger = ref<ErrorTrigger>()
const search = ref(''), plan = ref(''), state = ref(''), group = ref(''), resultFilter = ref(''), page = ref(1), pageSize = ref(100), selected = ref<string[]>([])
const queueOnly = ref(false)
const resultOptions = [{value:'',label:'全部检测结果'},{value:'degraded',label:'降智'},{value:'no_match',label:'未降智'},{value:'other',label:'其他'}]
const resultCategory = (job?: Job) => job?.status === 'completed' && ['degraded','no_match'].includes(job.verdict) ? job.verdict : 'other'
const historySearch = ref(''), historyVerdict = ref(''), historyPage = ref(1), historySize = ref(20), historySelected = ref<string[]>([])
const report = ref<Report>(), reportOpen = ref(false), roundIndex = ref(0), showAllModels = ref(false)
const confirmIds = ref<string[]>([]), confirmOpen = ref(false)
const deleteIds = ref<string[]>([]), deleteOpen = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined, clockTimer: ReturnType<typeof setInterval> | undefined, polling = false, disposed = false, failures = 0, progressRevision: string | undefined
const clock = ref(Date.now())
const requestIds = new Map<string,string>()
const plans = computed(() => [{ value:'',label:'全部订阅' }, ...Array.from(new Set(accounts.value.map(a => a.planType || 'unknown'))).sort().map(p => ({ value:p,label:p }))])
const groups = computed(() => [{ value:'',label:'全部分组' }, {value:'__ungrouped__',label:'未分组'}, ...Array.from(new Map(accounts.value.flatMap(a => a.groups).map(g => [g.id,g])).values()).map(g => ({value:g.id,label:g.name}))])
const statuses = computed(() => [{value:'',label:'全部账户状态'},{value:'disabled',label:'已停用'},...Array.from(new Set(accounts.value.map(a => a.status))).map(s => ({ value:s,label:statusLabels[s] || s }))])
const accountTone = (a: Account): Tone => !a.enabled ? 'neutral' : ({ normal:'success', error:'danger', quota_paused:'warning', quota_exhausted:'warning', rate_limited:'warning' } as Record<string,Tone>)[a.status] || 'info'
const latest = computed(() => { const result = new Map<string, Job>(); for (const job of jobs.value) if (!result.has(job.accountId)) result.set(job.accountId, job); return result })
const jobOf = (a: Account) => latest.value.get(a.id)
const manualActive = (a: Account) => jobs.value.some(j => j.accountId === a.id && j.source === 'manual' && active(j))
const canDetect = (a: Account) => a.enabled && a.supported && !manualActive(a)
const filtered = computed(() => accounts.value.filter(a => {
  const j = jobOf(a)
  return (!plan.value || (a.planType || 'unknown') === plan.value)
    && (!state.value || (state.value === 'disabled' ? !a.enabled : a.enabled && a.status === state.value))
    && (!group.value || (group.value === '__ungrouped__' ? !a.groups.length : a.groups.some(g => g.id === group.value)))
    && (!search.value.trim() || `${a.name} ${a.email || ''} ${a.id}`.toLowerCase().includes(search.value.trim().toLowerCase()))
    && (!queueOnly.value || jobs.value.some(j => j.accountId === a.id && active(j)))
    && (!resultFilter.value || (a.supported ? resultCategory(j) : 'other') === resultFilter.value)
}))
const visible = computed(() => filtered.value.slice((page.value-1)*pageSize.value,page.value*pageSize.value))
const pageCandidates = computed(() => visible.value.filter(canDetect))
const chosen = computed(() => accounts.value.filter(a => selected.value.includes(a.id) && canDetect(a)))
const allPage = computed(() => pageCandidates.value.length > 0 && pageCandidates.value.every(a => selected.value.includes(a.id)))
const activeJobs = computed(() => jobs.value.filter(j => active(j)).length)
const manualJobs = computed(() => jobs.value.filter(j => j.source === 'manual' && active(j)).length)
const scheduledJobs = computed(() => jobs.value.filter(j => j.source === 'schedule' && active(j)).length)
const triggeredJobs = computed(() => jobs.value.filter(j => j.source === 'error_code' && active(j)).length)
const runningJobs = computed(() => jobs.value.filter(j => j.status === 'running').length)
const queueBreakdown = computed(() => [['手动', manualJobs.value], ['定时', scheduledJobs.value], ['日志触发', triggeredJobs.value]].filter(([, n]) => n).map(([label, n]) => `${label} ${n}`).join(' · '))
const detectable = computed(() => accounts.value.filter(a => a.enabled && a.supported).length)
const disabledCount = computed(() => accounts.value.filter(a => !a.enabled).length)
const degradedCount = computed(() => accounts.value.filter(a => a.supported && resultCategory(jobOf(a)) === 'degraded').length)
const noMatchCount = computed(() => accounts.value.filter(a => a.supported && resultCategory(jobOf(a)) === 'no_match').length)
const untestedCount = computed(() => accounts.value.filter(a => a.supported && !jobOf(a)).length)
const listMode = computed(() => queueOnly.value ? 'queue' : resultFilter.value === 'degraded' ? 'degraded' : resultFilter.value ? '' : 'all')
const autoPeriod = computed(() => {
  const s = config.value?.schedule
  if (!s?.enabled) return '已关闭'
  if (s.kind === 'daily') return `每日 ${s.time}`
  const m = s.intervalMinutes
  if (m < 60) return `每 ${m} 分钟`
  if (m % 60 === 0) return `每 ${m / 60} 小时`
  return `每 ${Math.floor(m / 60)} 小时 ${m % 60} 分`
})
const autoTitle = computed(() => {
  const s = config.value?.schedule
  if (!s?.enabled) return '定时检测已关闭，可在参数配置中开启'
  return s.kind === 'daily' ? `定时检测：每日 ${s.time}（${s.timezone}）` : `定时检测：每 ${s.intervalMinutes} 分钟`
})
const pad = (n: number) => String(n).padStart(2, '0')
const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
function shortWhen(value: number) {
  const at = new Date(value), now = new Date(clock.value)
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`
  const days = Math.round((dayStart(at) - dayStart(now)) / 86_400_000)
  if (days === 0) return time
  if (days === 1) return `明天 ${time}`
  return `${at.getMonth() + 1}月${at.getDate()}日 ${time}`
}
/** Compact past timestamp for dense lists; the full value goes into a tooltip. */
function when(value?: number | null) {
  if (!value) return '—'
  const diff = clock.value - value
  if (diff >= 0 && diff < 45_000) return '刚刚'
  if (diff >= 0 && diff < 3_600_000) return `${Math.max(1, Math.round(diff / 60_000))} 分钟前`
  const at = new Date(value), now = new Date(clock.value)
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`
  const days = Math.round((dayStart(now) - dayStart(at)) / 86_400_000)
  if (days === 0) return `今天 ${time}`
  if (days === 1) return `昨天 ${time}`
  if (at.getFullYear() === now.getFullYear()) return `${at.getMonth() + 1}月${at.getDate()}日 ${time}`
  return `${at.getFullYear()}/${at.getMonth() + 1}/${at.getDate()}`
}
const nextRun = computed(() => config.value?.schedule.enabled ? nextDue.value : null)
const runWhen = computed(() => nextRun.value == null ? '—' : nextRun.value <= clock.value + 2000 ? '现在' : shortWhen(nextRun.value))
const triggerTone = computed<Tone>(() => !errorTrigger.value?.enabled ? 'neutral' : errorTrigger.value.error ? 'warning' : 'success')

const isActive = (s: string) => ['queued','running'].includes(s)
const shownVerdict = (j: Job | Report) => isActive(j.status) || j.status !== 'completed' ? j.status : j.verdict
const verdictTone = (v: string): Tone => ({ degraded:'danger', suspected:'warning', low_confidence:'warning', inconclusive:'neutral', no_match:'success', queued:'info', running:'primary', failed:'warning', interrupted:'warning', cancelled:'neutral' } as Record<string,Tone>)[v] || 'neutral'
const sourceLabel = (source?: string, code?: number | null) => source === 'schedule' ? '定时' : source === 'error_code' ? `日志触发${code ? ` ${code}` : ''}` : '手动'
const degradedModels = computed(() => new Set(config.value?.degradedModels ?? []))

const history = computed(() => jobs.value.filter(j => (!historySearch.value || `${j.accountName} ${j.accountId}`.toLowerCase().includes(historySearch.value.toLowerCase())) && (!historyVerdict.value || resultCategory(j) === historyVerdict.value)))
const historyCounts = computed(() => { const c: Record<string, number> = { degraded:0, no_match:0, other:0 }; for (const j of jobs.value) { const k = resultCategory(j); c[k] = (c[k] ?? 0) + 1 } return c })
const historyDeletable = (j: Job) => !active(j) && !['pending','attempting'].includes(j.disableState) && !['pending','attempting'].includes(j.groupState)
const historyCandidates = computed(() => history.value.filter(historyDeletable))
const selectedHistory = computed(() => jobs.value.filter(j => historySelected.value.includes(j.id) && historyDeletable(j)))
const allHistory = computed(() => historyCandidates.value.length > 0 && historyCandidates.value.every(j => historySelected.value.includes(j.id)))
const visibleHistory = computed(() => history.value.slice((historyPage.value-1)*historySize.value,historyPage.value*historySize.value))
const currentRound = computed(() => report.value?.rounds[roundIndex.value])
watch([search,plan,state,group,resultFilter,queueOnly,pageSize], () => { page.value=1 })
watch([historySearch,historyVerdict,historySize], () => { historyPage.value=1 })
watch(filtered, a => { page.value=Math.min(page.value,Math.max(1,Math.ceil(a.length/pageSize.value))) })
watch(history, a => { historyPage.value=Math.min(historyPage.value,Math.max(1,Math.ceil(a.length/historySize.value))) })
watch(() => report.value?.id, () => { showAllModels.value=false })

const columns: TableColumn<Account>[] = [{key:'select',label:'',kind:'selection',width:48},{key:'identity',label:'账户',kind:'identity',minWidth:250},{key:'plan',label:'订阅',kind:'custom',width:90},{key:'groups',label:'分组',kind:'custom',minWidth:140},{key:'status',label:'账户状态',kind:'custom',width:108},{key:'result',label:'检测结果',kind:'custom',minWidth:280},{key:'time',label:'检测时间',kind:'custom',width:150},{key:'actions',label:'操作',kind:'actions',width:132}]
const historyColumns: TableColumn<Job>[] = [{key:'select',label:'',kind:'selection',width:48},{key:'account',label:'账户',kind:'identity',minWidth:230},{key:'result',label:'结论',kind:'custom',minWidth:240},{key:'model',label:'最高候选',kind:'custom',minWidth:200},{key:'createdAt',label:'检测时间',kind:'custom',width:150},{key:'actions',label:'操作',kind:'actions',width:132}]

function selectOne(id:string,checked:boolean) { selected.value=checked ? Array.from(new Set([...selected.value,id])) : selected.value.filter(x => x !== id) }
function selectPage(checked:boolean) { const ids=pageCandidates.value.map(a => a.id); selected.value=checked ? Array.from(new Set([...selected.value,...ids])) : selected.value.filter(id => !ids.includes(id)) }
function selectHistoryOne(id:string,checked:boolean) { historySelected.value=checked ? Array.from(new Set([...historySelected.value,id])) : historySelected.value.filter(x => x !== id) }
function selectHistoryAll(checked:boolean) { const ids=historyCandidates.value.map(j => j.id); historySelected.value=checked ? Array.from(new Set([...historySelected.value,...ids])) : historySelected.value.filter(id => !ids.includes(id)) }
async function refreshJobs() { const r=await api.jobs(); jobs.value=r.jobs; banks.value=r.banks; nextDue.value=r.nextDue; scheduleError.value=r.scheduleError; errorTrigger.value=r.errorTrigger ?? errorTrigger.value; progressRevision=undefined }
function progressText(j: Job | Report) {
  const p=j.progress
  if (!p || !active(j as Job)) return j.message
  const labels: Record<string,string>={preparing:'正在准备挑战',requesting:'正在发送请求',waiting_response:'等待模型响应',receiving:'正在接收回答',waiting_validation:'已保存回答，等待校验',validating:'正在校验样本',waiting_score:'已获得有效样本，等待评分',scoring:'正在分析结果',waiting_retry:'等待重试',waiting_retest:'等待复测',cancelling:'正在取消检测',waiting_request:'准备完成，等待发起请求'}
  const remaining=p.detail.nextAt ? Math.max(0,Math.ceil((p.detail.nextAt-clock.value)/1000)) : 0
  const attemptLimit='config' in j ? j.config.failureRetries+1 : j.attemptLimit ?? 5
  if (p.stage==='waiting_retry' || p.stage==='waiting_retest') return `${j.message || labels[p.stage]}${j.roundAttempts ? `（本轮已尝试 ${j.roundAttempts}/${attemptLimit} 次）` : ''} · ${remaining>0 ? `${remaining} 秒后可执行` : '等待调度'}`
  const stale=Math.max(0,Math.floor((clock.value-(p.updatedAt || p.stageStartedAt))/1000))
  if (p.stage==='receiving' && stale>=8) return `进度已过期（${stale} 秒未更新），等待后台恢复`
  if (stale>=90) return `进度长时间未更新（${stale} 秒），等待后台恢复`
  const elapsed=Math.max(0,Math.floor((clock.value-p.stageStartedAt)/1000))
  const d=p.detail
  const traffic=d.outputBytes != null ? ` · 正文 ${(d.outputBytes/1024).toFixed(1)} KB` : d.receivedBytes ? ` · 流数据 ${(d.receivedBytes/1024).toFixed(1)} KB` : ''
  return `${labels[p.stage] || j.message} · ${elapsed} 秒${traffic}`
}
/** Secondary line under a verdict: live progress, failures and follow-up
    actions. Parts that only restate the verdict are dropped. */
const verdictLabels = new Set(['degraded','suspected','low_confidence','inconclusive','no_match','queued','running','failed','interrupted','cancelled','completed'].flatMap(v => [verdict(v), `已${verdict(v)}`]))
function detailText(j: Job | Report) {
  if (isActive(j.status)) return progressText(j)
  return (j.message || '').split('；').map(s => s.trim()).filter(s => s && !verdictLabels.has(s)).join('；')
}
const detailClass = (j: Job | Report) => j.status === 'failed' || /失败/.test(j.message || '') ? 'text-danger' : ''
function diagText(d: Diagnostics) {
  return `耗时 ${(d.elapsedMs/1000).toFixed(1)} 秒${d.outputBytes != null ? ` · 正文 ${(d.outputBytes/1024).toFixed(1)} KB` : ''} · 流数据 ${(d.receivedBytes/1024).toFixed(1)} KB${d.budgetMs != null ? ` · 本次请求窗口 ${(d.budgetMs/1000).toFixed(1)} 秒` : ''}`
}

const reportTop = computed(() => {
  const s = report.value?.rounds.at(-1)?.score
  if (!s) return undefined
  return { label: s.results.find(r => r.model === s.prediction)?.label || s.prediction, probability: s.probability }
})
const thresholdShown = computed(() => { const p = report.value?.config?.degradedPercent ?? 0; return p > 0 && p < 100 })
const reportDegraded = computed(() => new Set(report.value?.config?.degradedModels ?? []))
const sortedResults = computed(() => [...(currentRound.value?.score.results ?? [])].sort((a, b) => b.probability - a.probability))
const TOP_MODELS = 8
const shownResults = computed(() => showAllModels.value ? sortedResults.value : sortedResults.value.filter((r, i) => i < TOP_MODELS || reportDegraded.value.has(r.model)))
const hiddenModels = computed(() => sortedResults.value.length - sortedResults.value.filter((r, i) => i < TOP_MODELS || reportDegraded.value.has(r.model)).length)
const snapshot = computed(() => {
  const r = report.value, c = r?.config
  if (!r || !c) return []
  return [
    ['请求模型', c.requestModel], ['推理强度', c.reasoningEffort || '默认'], ['温度', c.temperature ?? '默认'], ['最大输出', `${c.maxOutputTokens} tokens`],
    ['失败后重试', `${c.failureRetries} 次`], ['请求被拒绝后', c.requestRejectedAction==='disable' ? '停止账户调度' : '保留调度，下次继续检测'],
    ['单次请求超时', `${c.timeoutSeconds} 秒`], ['降智判定阈值', `${c.degradedPercent}%`], ['低置信度阈值', `${c.lowConfidencePercent}%`], ['连续确认轮数', `${c.confirmationRounds} 轮`],
    ['低置信度复测', `${c.lowConfidenceRetries} 次`], ['降智模型', c.degradedModels?.join('、') || '—'], ['指纹库提交', r.bankCommit || '—'], ['引擎提交', r.engineCommit || '—'],
  ] as [string, string | number][]
})
const confirmAccounts = computed(() => confirmIds.value.map(id => accounts.value.find(a => a.id === id)).map(a => a ? a.email || a.name : ''))
const keyName = computed(() => keys.value.find(k => k.id === config.value?.keyId)?.name || config.value?.keyId || '—')

async function refreshProgress() {
  const r=await api.progress(progressRevision)
  if (disposed) return
  progressRevision=r.revision; nextDue.value=r.nextDue; scheduleError.value=r.scheduleError; errorTrigger.value=r.errorTrigger ?? errorTrigger.value
  if (!r.changed || !r.jobs) return
  const previousActions=new Map(jobs.value.map(j=>[j.id,`${j.disableState}:${j.groupState}`]))
  jobs.value=r.jobs
  if (r.jobs.some(j=>(j.disableState==='disabled' || j.groupState==='moved') && previousActions.get(j.id)!==`${j.disableState}:${j.groupState}`)) await loadAccounts()
  const current=report.value
  if (reportOpen.value && current) {
    const job=r.jobs.find(j=>j.id===current.id)
    if (job && (job.attemptCount!==current.attempts.length || job.roundCount!==current.rounds.length || job.status!==current.status || job.disableState!==current.disableState || job.groupState!==current.groupState)) {
      const updated=await api.report(current.id)
      if (reportOpen.value && report.value?.id===current.id) {
        const followingLatest=roundIndex.value>=current.rounds.length-1
        report.value=updated
        if (followingLatest) roundIndex.value=Math.max(0,updated.rounds.length-1)
      }
    } else if (job && report.value?.id===current.id) {
      report.value={...current,progress:job.progress,message:job.message}
    }
  }
}
function schedulePoll(delay?: number) {
  if (timer) clearTimeout(timer)
  if (disposed || document.hidden) return
  timer=setTimeout(poll,delay ?? (failures ? Math.min(30000,1000*2**failures) : (activeJobs.value || jobs.value.some(j=>[j.disableState,j.groupState].some(s=>['pending','attempting'].includes(s)))) ? 1000 : 10000))
}
async function poll() {
  if (disposed || document.hidden || polling) return
  polling=true
  try { await refreshProgress(); failures=0 } catch(e) { failures++; if (failures>=3) error.value=(e as Error).message }
  finally { polling=false; schedulePoll() }
}
function visibilityChanged() { if (document.hidden) {if(timer) clearTimeout(timer)} else {clock.value=Date.now(); schedulePoll(0)} }

async function loadAccounts() { accounts.value=(await api.accounts()).accounts }
async function action(fn: () => Promise<void>) { if(busy.value) return; busy.value=true; error.value=''; notice.value=''; try { await fn() } catch(e) { error.value=(e as Error).message } finally { busy.value=false; schedulePoll(0) } }
async function load() { loading.value=true; try { const [b,a]=await Promise.all([api.bootstrap(),api.accounts()]); config.value=b.config; if(!b.configRevision) config.value.schedule.timezone=Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'; revision.value=b.configRevision; banks.value=b.banks; keys.value=b.keys; allGroups.value=b.groups; jobs.value=b.jobs; accounts.value=a.accounts; nextDue.value=b.nextDue; scheduleError.value=b.scheduleError; errorTrigger.value=b.errorTrigger; settingsKey.value++ } catch(e) { error.value=(e as Error).message } finally { loading.value=false } }
function openSubmit(ids:string[]) { confirmIds.value=ids; confirmOpen.value=true }
async function submit() { await action(async () => { const signature=[...confirmIds.value].sort().join(','); let id=requestIds.get(signature); if(!id) { id=createRequestId();requestIds.set(signature,id) } await api.submit(confirmIds.value,id); requestIds.delete(signature); confirmOpen.value=false; selected.value=selected.value.filter(id => !confirmIds.value.includes(id)); await refreshJobs(); notice.value='检测已加入后台队列，离开页面后仍会继续。' }) }
async function save(c:Config) { await action(async () => { const r=await api.save(c,revision.value); config.value=r.config; revision.value=r.revision; settingsKey.value++; await refreshJobs(); notice.value='参数已保存。' }) }
async function bank(operation:'check'|'apply'|'rollback'|'builtin') { await action(async () => { banks.value=await api.bank(operation); notice.value=operation==='check' ? (banks.value.update.error ? '' : banks.value.update.available ? '发现可用更新。' : '当前指纹库已是最新内容。') : '指纹库版本已切换，新任务将使用此版本。' }) }
async function openReport(id:string) { await action(async () => { report.value=await api.report(id); roundIndex.value=0; reportOpen.value=true }) }
async function cancel(id?:string, source?:'manual'|'schedule'|'error_code') { await action(async () => { await api.cancel(id,source); await refreshJobs() }) }
async function remove(id:string) { await action(async () => { await api.delete(id); historySelected.value=historySelected.value.filter(x=>x!==id); await refreshJobs() }) }
function openDelete(ids:string[]) { deleteIds.value=ids; deleteOpen.value=true }
async function removeSelected() { await action(async () => { const ids=deleteIds.value; const r=await api.deleteMany(ids); deleteOpen.value=false; historySelected.value=historySelected.value.filter(id=>!ids.includes(id)); await refreshJobs(); notice.value=`已删除 ${r.deleted} 条检测记录${r.skipped ? `，${r.skipped} 条正在执行或处理操作已跳过` : ''}。` }) }
async function retryDisable(id:string) { await action(async () => { await api.retryDisable(id); await refreshJobs(); await loadAccounts(); if(report.value?.id===id) report.value=await api.report(id) }) }
async function retryGroup(id:string) { await action(async () => { await api.retryGroup(id); await refreshJobs(); await loadAccounts(); if(report.value?.id===id) report.value=await api.report(id) }) }
onMounted(async () => { document.addEventListener('visibilitychange',visibilityChanged); clockTimer=setInterval(()=>{clock.value=Date.now()},1000); await load(); schedulePoll(0) })
onUnmounted(() => { disposed=true; if(timer) clearTimeout(timer); if(clockTimer) clearInterval(clockTimer); document.removeEventListener('visibilitychange',visibilityChanged) })
</script>

<template>
  <main class="app">
    <header class="topbar">
      <nav class="nav-tabs" aria-label="检测插件导航">
        <button v-for="t in tabs" :key="t.key" type="button" class="nav-tab" :class="{ active: tab===t.key }" :aria-current="tab===t.key ? 'page' : undefined" @click="tab=t.key"><component :is="t.icon" :size="16" aria-hidden="true" /><span>{{ t.label }}</span></button>
      </nav>
      <span v-if="banks" class="bank-chip" :title="`指纹库提交 ${banks.active.commit}`"><Fingerprint :size="14" aria-hidden="true" />指纹库 <code class="mono">{{ banks.active.commit.slice(0,7) }}</code><span aria-hidden="true">·</span>{{ banks.active.models.length }} 个模型</span>
    </header>

    <div v-if="error" class="notice tone-danger flash" role="alert"><CircleAlert :size="16" aria-hidden="true" /><span>{{ error }}</span><button type="button" class="flash-close" aria-label="关闭错误提示" @click="error=''"><X :size="15" /></button></div>
    <div v-if="notice" class="notice tone-success flash" role="status"><CircleCheck :size="16" aria-hidden="true" /><span>{{ notice }}</span><button type="button" class="flash-close" aria-label="关闭提示" @click="notice=''"><X :size="15" /></button></div>
    <p v-if="loading" class="loading-line">正在读取账户与插件配置…</p>

    <template v-if="tab==='accounts'">
      <section class="overview" aria-label="账户概览">
        <button type="button" class="ov-tile ov-filter tone-primary" :class="{ active: listMode==='all' }" :aria-pressed="listMode==='all'" :aria-label="`全部账户 ${accounts.length}`" aria-describedby="ov-all-foot" @click="resultFilter='';queueOnly=false">
          <span class="ov-head"><span class="ov-icon"><Users :size="15" /></span><span class="ov-label">全部账户</span></span>
          <span class="ov-value tabular">{{ accounts.length }}</span>
          <span id="ov-all-foot" class="ov-foot">可检测 {{ detectable }} · 已停用 {{ disabledCount }}</span>
        </button>
        <button type="button" class="ov-tile ov-filter" :class="[{ active: listMode==='queue' }, activeJobs ? 'tone-primary' : '']" :aria-pressed="listMode==='queue'" :aria-label="`检测队列 ${activeJobs}`" aria-describedby="ov-queue-foot" @click="resultFilter='';queueOnly=true">
          <span class="ov-head"><span class="ov-icon"><Activity :size="15" /></span><span class="ov-label">检测队列</span></span>
          <span class="ov-value tabular">{{ activeJobs }}</span>
          <span id="ov-queue-foot" class="ov-foot">{{ activeJobs ? `进行中 ${runningJobs} · 排队 ${activeJobs - runningJobs}` : '当前空闲' }}</span>
        </button>
        <button type="button" class="ov-tile ov-filter" :class="[{ active: listMode==='degraded' }, degradedCount ? 'tone-danger' : '']" :aria-pressed="listMode==='degraded'" :aria-label="`确认降智 ${degradedCount}`" aria-describedby="ov-degraded-foot" @click="resultFilter='degraded';queueOnly=false">
          <span class="ov-head"><span class="ov-icon"><ShieldAlert :size="15" /></span><span class="ov-label">确认降智</span></span>
          <span class="ov-value tabular">{{ degradedCount }}</span>
          <span id="ov-degraded-foot" class="ov-foot">未降智 {{ noMatchCount }} · 未检测 {{ untestedCount }}</span>
        </button>
        <div class="ov-tile ov-status schedule-tile" :class="config?.schedule.enabled ? 'tone-success' : ''">
          <span class="ov-head"><span class="ov-icon"><CalendarClock :size="15" /></span><span class="ov-label">定时检测</span><span v-if="config?.schedule.enabled" class="pill dot tone-success">已开启</span></span>
          <span class="ov-value ov-text" :class="{ off: !config?.schedule.enabled }" :title="autoTitle">{{ autoPeriod }}</span>
          <span class="ov-foot next-run" :title="nextRun != null ? `下次自动检测：${date(nextRun)}` : '当前没有待执行的自动检测'">下次检测 <b>{{ runWhen }}</b></span>
          <span v-if="scheduleError" class="ov-foot text-warning" :title="scheduleError">{{ scheduleError }}</span>
        </div>
        <div class="ov-tile ov-status trigger-tile" :class="`tone-${triggerTone}`">
          <span class="ov-head"><span class="ov-icon"><Zap :size="15" /></span><span class="ov-label">日志触发</span><span v-if="errorTrigger?.enabled" class="pill dot" :class="`tone-${triggerTone}`">{{ errorTrigger.error ? '异常' : '监听中' }}</span></span>
          <template v-if="errorTrigger?.enabled">
            <span class="ov-value ov-codes" :title="`请求日志出现这些状态码时自动检测对应账户`"><span v-for="c in errorTrigger.codes" :key="c" class="code-chip mono">{{ c }}</span></span>
            <span v-if="errorTrigger.error" class="ov-foot text-warning" :title="errorTrigger.error"><TriangleAlert :size="12" aria-hidden="true" /> {{ errorTrigger.error }}</span>
            <span v-else class="ov-foot" :title="errorTrigger.checkedAt ? `最近检查：${date(errorTrigger.checkedAt)}` : undefined">{{ errorTrigger.checkedAt ? `最近检查 ${when(errorTrigger.checkedAt)}` : '等待首次检查' }} · 累计触发 <b>{{ errorTrigger.matchedTotal }}</b> 次</span>
            <span class="ov-foot" :title="errorTrigger.matchedAt ? `最近命中：${errorTrigger.matchedAccount || '—'} · ${errorTrigger.matchedCode} · ${date(errorTrigger.matchedAt)}` : undefined">{{ errorTrigger.matchedAt ? `最近命中 ${errorTrigger.matchedAccount || '—'} · ${errorTrigger.matchedCode} · ${when(errorTrigger.matchedAt)}` : '暂无命中记录' }}</span>
          </template>
          <template v-else>
            <span class="ov-value ov-text off">未开启</span>
            <span class="ov-foot">在参数配置中填写触发错误码后启用</span>
          </template>
        </div>
      </section>

      <section class="panel list-panel">
        <div class="list-toolbar">
          <div class="list-filters">
            <ZInput v-model="search" class="list-search" placeholder="搜索名称、邮箱或账户ID" clearable aria-label="搜索账户"><template #prefix><Search :size="15" /></template></ZInput>
            <ZSelect v-model="plan" class="list-filter" :options="plans" aria-label="订阅类型筛选" />
            <ZSelect v-model="state" class="list-filter" :options="statuses" aria-label="账户状态筛选" />
            <ZSelect v-model="group" class="list-filter" :options="groups" aria-label="账户分组筛选" />
            <ZSelect v-model="resultFilter" class="list-filter" :options="resultOptions" aria-label="检测结果筛选" @change="queueOnly=false" />
          </div>
          <div class="list-actions">
            <ZButton :disabled="busy" @click="action(loadAccounts)"><template #icon><RefreshCw :size="15" /></template>刷新账户</ZButton>
            <ZButton type="primary" :disabled="busy || !chosen.length" @click="openSubmit(chosen.map(a=>a.id))"><template #icon><Play :size="15" /></template>检测所选（{{ chosen.length }}）</ZButton>
          </div>
        </div>
        <div v-if="activeJobs" class="queue-bar">
          <span class="queue-text"><Activity :size="15" aria-hidden="true" /><span>队列中 <b class="tabular">{{ activeJobs }}</b> 个检测任务</span><span class="queue-parts">{{ queueBreakdown }}</span></span>
          <span class="queue-actions">
            <ZButton v-if="manualJobs" size="small" :disabled="busy" @click="cancel(undefined,'manual')"><template #icon><Square :size="12" /></template>停止手动队列</ZButton>
            <ZButton v-if="scheduledJobs" size="small" :disabled="busy" @click="cancel(undefined,'schedule')"><template #icon><Square :size="12" /></template>停止自动队列</ZButton>
            <ZButton v-if="triggeredJobs" size="small" :disabled="busy" @click="cancel(undefined,'error_code')"><template #icon><Square :size="12" /></template>停止日志触发队列</ZButton>
          </span>
        </div>
        <div class="selection-bar">
          <ZCheckbox :model-value="allPage" :disabled="!pageCandidates.length || busy" label="选择当前页" @change="selectPage" />
          <span class="selection-count">已选 <b>{{ chosen.length }}</b> 个 · 匹配 <b>{{ filtered.length }}</b> 个</span>
          <ZButton v-if="selected.length" size="small" variant="text" @click="selected=[]">清空选择</ZButton>
          <ZButton v-if="resultFilter || queueOnly" size="small" variant="text" @click="resultFilter='';queueOnly=false">清除结果筛选</ZButton>
        </div>
        <ZTable :columns="columns" :data="visible" :row-key="a=>a.id" :loading="loading" empty-text="没有匹配的账户">
          <template #select="{row}"><ZCheckbox :model-value="selected.includes(row.id)" :disabled="!canDetect(row) || busy" :aria-label="`选择 ${row.email || row.name}`" @change="checked=>selectOne(row.id,checked)" /></template>
          <template #identity="{row}"><div class="identity"><span class="avatar" aria-hidden="true">{{ (row.email || row.name || '?').slice(0,1).toUpperCase() }}</span><div><b :title="row.email || row.name">{{ row.email || row.name }}</b><small :title="row.id">{{ row.name }} · {{ row.provider }}</small></div></div></template>
          <template #plan="{row}"><span class="pill plan-pill">{{ row.planType || '未知' }}</span></template>
          <template #groups="{row}"><div class="tag-list"><span v-for="g in row.groups" :key="g.id" class="pill group-pill">{{ g.name }}</span><span v-if="!row.groups.length" class="cell-empty">未分组</span></div></template>
          <template #status="{row}"><span class="pill dot account-status" :class="`tone-${accountTone(row)}`">{{ status(row) }}</span></template>
          <template #result="{row}">
            <span v-if="!row.supported" class="cell-empty">暂不支持此提供方</span>
            <div v-else-if="jobOf(row)" class="cell-stack">
              <div class="cell-line">
                <span class="pill dot" :class="[`tone-${verdictTone(shownVerdict(jobOf(row)!))}`, { pulse: active(jobOf(row)) }]">{{ verdict(shownVerdict(jobOf(row)!)) }}</span>
                <span v-if="jobOf(row)!.prediction" class="cand-inline" :class="{ 'is-degraded': degradedModels.has(jobOf(row)!.prediction!) }" :title="`最高候选 ${jobOf(row)!.prediction} · ${percent(jobOf(row)!.probability)}`"><span class="mono">{{ jobOf(row)!.prediction }}</span><b class="tabular">{{ percent(jobOf(row)!.probability) }}</b></span>
              </div>
              <span v-if="detailText(jobOf(row)!)" class="cell-sub live-progress" :class="detailClass(jobOf(row)!)" role="status" :title="detailText(jobOf(row)!)">{{ detailText(jobOf(row)!) }}</span>
            </div>
            <span v-else class="cell-empty">尚未检测</span>
          </template>
          <template #time="{row}">
            <div v-if="jobOf(row)" class="cell-stack">
              <time class="cell-time tabular" :title="date(jobOf(row)!.finishedAt || jobOf(row)!.createdAt)">{{ when(jobOf(row)!.finishedAt || jobOf(row)!.createdAt) }}</time>
              <span class="src-pill detection-source" :class="`src-${jobOf(row)!.source}`">{{ sourceLabel(jobOf(row)!.source, jobOf(row)!.triggerCode) }}</span>
            </div>
            <span v-else class="cell-empty">—</span>
          </template>
          <template #actions="{row}"><div class="row-actions"><ZButton size="small" :disabled="!canDetect(row) || busy" @click="openSubmit([row.id])">检测</ZButton><ZButton v-if="jobOf(row)" size="small" variant="text" :disabled="busy" @click="openReport(jobOf(row)!.id)">报告</ZButton></div></template>
        </ZTable>
        <div class="list-footer"><ZPagination v-model:current-page="page" v-model:page-size="pageSize" :total="filtered.length" :page-sizes="[10,20,50,100]" /></div>
      </section>
    </template>

    <Settings v-if="tab==='settings' && config && banks" :key="settingsKey" :config="config" :revision="revision" :banks="banks" :keys="keys" :accounts="accounts" :groups="allGroups" :selected="selected" :next-due="nextDue" :schedule-error="scheduleError" :error-trigger="errorTrigger" :busy="busy" @save="save" @bank="bank" @error="error=$event" />

    <template v-if="tab==='history'">
      <section class="overview overview-history" aria-label="检测记录概览">
        <button type="button" class="ov-tile ov-filter tone-primary" :class="{ active: !historyVerdict }" :aria-pressed="!historyVerdict" :aria-label="`全部记录 ${jobs.length}`" @click="historyVerdict=''">
          <span class="ov-head"><span class="ov-icon"><ListChecks :size="15" /></span><span class="ov-label">全部记录</span></span>
          <span class="ov-value tabular">{{ jobs.length }}</span>
        </button>
        <button type="button" class="ov-tile ov-filter" :class="[{ active: historyVerdict==='degraded' }, historyCounts.degraded ? 'tone-danger' : '']" :aria-pressed="historyVerdict==='degraded'" :aria-label="`降智记录 ${historyCounts.degraded}`" @click="historyVerdict='degraded'">
          <span class="ov-head"><span class="ov-icon"><ShieldAlert :size="15" /></span><span class="ov-label">降智</span></span>
          <span class="ov-value tabular">{{ historyCounts.degraded }}</span>
        </button>
        <button type="button" class="ov-tile ov-filter tone-success" :class="{ active: historyVerdict==='no_match' }" :aria-pressed="historyVerdict==='no_match'" :aria-label="`未降智记录 ${historyCounts.no_match}`" @click="historyVerdict='no_match'">
          <span class="ov-head"><span class="ov-icon"><CircleCheck :size="15" /></span><span class="ov-label">未降智</span></span>
          <span class="ov-value tabular">{{ historyCounts.no_match }}</span>
        </button>
        <button type="button" class="ov-tile ov-filter" :class="{ active: historyVerdict==='other' }" :aria-pressed="historyVerdict==='other'" :aria-label="`其他记录 ${historyCounts.other}`" @click="historyVerdict='other'">
          <span class="ov-head"><span class="ov-icon"><CircleAlert :size="15" /></span><span class="ov-label" title="进行中、疑似、证据不足、失败或已取消">其他</span></span>
          <span class="ov-value tabular">{{ historyCounts.other }}</span>
        </button>
      </section>
      <section class="panel list-panel">
        <div class="list-toolbar">
          <div class="list-filters">
            <ZInput v-model="historySearch" class="list-search" placeholder="搜索账户名称或ID" clearable aria-label="搜索历史账户"><template #prefix><Search :size="15" /></template></ZInput>
            <ZSelect v-model="historyVerdict" class="list-filter" :options="resultOptions" aria-label="历史结论筛选" />
          </div>
          <div class="list-actions">
            <span class="list-hint">保留各轮概率、参数及指纹库版本</span>
            <ZButton :disabled="busy" @click="action(refreshJobs)"><template #icon><RefreshCw :size="15" /></template>刷新</ZButton>
          </div>
        </div>
        <div class="selection-bar">
          <ZCheckbox :model-value="allHistory" :disabled="!historyCandidates.length || busy" :label="allHistory ? '取消全选' : `全选筛选结果（${historyCandidates.length}）`" @change="selectHistoryAll" />
          <span class="selection-count">已选 <b>{{ selectedHistory.length }}</b> 条 · 匹配 <b>{{ history.length }}</b> 条</span>
          <ZButton v-if="historySelected.length" size="small" variant="text" @click="historySelected=[]">清空选择</ZButton>
          <ZButton v-if="historyVerdict" size="small" variant="text" @click="historyVerdict=''">清除结论筛选</ZButton>
          <span class="selection-tail"><ZButton type="danger" size="small" :disabled="busy || !selectedHistory.length" @click="openDelete(selectedHistory.map(j=>j.id))">删除所选（{{ selectedHistory.length }}）</ZButton></span>
        </div>
        <ZTable :columns="historyColumns" :data="visibleHistory" :row-key="j=>j.id" empty-text="还没有检测记录">
          <template #select="{row}"><ZCheckbox :model-value="historySelected.includes(row.id)" :disabled="!historyDeletable(row) || busy" :aria-label="`选择 ${row.accountName} 的检测记录 ${date(row.createdAt)}`" @change="checked=>selectHistoryOne(row.id,checked)" /></template>
          <template #account="{row}"><div class="identity"><span class="avatar" aria-hidden="true">{{ (row.accountName || '?').slice(0,1).toUpperCase() }}</span><div><b :title="row.accountName">{{ row.accountName }}</b><small class="mono" :title="row.id">{{ row.accountId }}</small></div></div></template>
          <template #result="{row}">
            <div class="cell-stack">
              <span class="pill dot" :class="[`tone-${verdictTone(shownVerdict(row))}`, { pulse: active(row) }]">{{ verdict(shownVerdict(row)) }}</span>
              <span v-if="detailText(row)" class="cell-sub" :class="detailClass(row)" :title="detailText(row)">{{ detailText(row) }}</span>
            </div>
          </template>
          <template #model="{row}">
            <div v-if="row.prediction" class="cand" :class="{ 'is-degraded': degradedModels.has(row.prediction) }">
              <span class="cand-name mono" :title="row.prediction">{{ row.prediction }}</span>
              <span class="cand-meter"><span class="meter" aria-hidden="true"><i :style="{ width: `${(row.probability ?? 0) * 100}%` }" /></span><b class="tabular">{{ percent(row.probability) }}</b></span>
            </div>
            <span v-else class="cell-empty">—</span>
          </template>
          <template #createdAt="{row}">
            <div class="cell-stack">
              <time class="cell-time tabular" :title="date(row.createdAt)">{{ when(row.createdAt) }}</time>
              <span class="src-pill detection-source" :class="`src-${row.source}`">{{ sourceLabel(row.source, row.triggerCode) }}</span>
            </div>
          </template>
          <template #actions="{row}"><div class="row-actions"><ZButton size="small" variant="text" :disabled="busy" @click="openReport(row.id)">报告</ZButton><ZButton v-if="active(row)" size="small" variant="text" :disabled="busy" @click="cancel(row.id)">取消</ZButton><ZButton v-else size="small" variant="text" :disabled="busy || !historyDeletable(row)" @click="remove(row.id)">删除</ZButton></div></template>
        </ZTable>
        <div class="list-footer"><ZPagination v-model:current-page="historyPage" v-model:page-size="historySize" :total="history.length" :page-sizes="[10,20,50,100]" /></div>
      </section>
    </template>

    <ZDialog v-model="confirmOpen" title="开始账户检测" width="34rem" :close-on-click-modal="!busy">
      <div class="dlg">
        <div class="dlg-summary">
          <div class="dlg-count"><b class="tabular">{{ confirmIds.length }}</b><span>个账户</span></div>
          <dl class="dlg-facts">
            <div><dt>请求模型</dt><dd class="mono">{{ config?.requestModel || '—' }}</dd></div>
            <div><dt>计量 Key</dt><dd>{{ keyName }}</dd></div>
          </dl>
        </div>
        <div v-if="confirmAccounts.length" class="dlg-accounts"><span v-for="(name,i) in confirmAccounts.slice(0,6)" :key="i" class="pill group-pill">{{ name }}</span><span v-if="confirmAccounts.length > 6" class="pill tone-neutral">另 {{ confirmAccounts.length - 6 }} 个</span></div>
        <ul class="dlg-notes"><li>每轮只检测一次，失败时最多尝试五次；确认与低置信度复测会追加轮次。</li><li>任务在后台排队执行，离开页面后仍会继续。</li></ul>
        <div v-if="config?.autoDisable" class="notice tone-warning"><TriangleAlert :size="15" aria-hidden="true" /><span>已开启：确认降智后自动关闭该账户调度。</span></div>
        <div v-if="config?.autoMoveGroup" class="notice tone-warning"><TriangleAlert :size="15" aria-hidden="true" /><span>已开启：确认降智后移入 {{ allGroups.find(g=>g.id===config?.targetGroupId)?.name || config?.targetGroupId }}，替换原有分组。</span></div>
        <div v-if="error" class="notice tone-danger"><CircleAlert :size="15" aria-hidden="true" /><span>{{ error }}</span></div>
      </div>
      <template #footer><ZButton :disabled="busy" @click="confirmOpen=false">取消</ZButton><ZButton type="primary" :loading="busy" @click="submit">加入检测队列</ZButton></template>
    </ZDialog>

    <ZDialog v-model="deleteOpen" title="删除检测历史" width="32rem" :close-on-click-modal="!busy">
      <div class="dlg">
        <div class="dlg-lead">确定删除已选的 <b class="tabular">{{ deleteIds.length }}</b> 条检测记录？</div>
        <ul class="dlg-notes"><li>只删除历史与报告，不会恢复已停用账户，也不会改变账户分组或调度状态。</li><li>正在执行或等待关闭调度、移组的记录不会被删除。</li></ul>
      </div>
      <template #footer><ZButton :disabled="busy" @click="deleteOpen=false">取消</ZButton><ZButton type="danger" :loading="busy" @click="removeSelected">删除 {{ deleteIds.length }} 条</ZButton></template>
    </ZDialog>

    <ZDialog v-model="reportOpen" title="检测报告" width="60rem">
      <div v-if="report" class="rpt">
        <section class="rpt-summary">
          <div class="rpt-who">
            <span class="avatar" aria-hidden="true">{{ (report.accountName || '?').slice(0,1).toUpperCase() }}</span>
            <div class="rpt-who-text">
              <b>{{ report.accountName }}</b>
              <span class="rpt-who-meta"><span class="src-pill" :class="`src-${report.source || 'manual'}`">{{ sourceLabel(report.source, report.trigger?.code) }}</span><span class="mono" :title="report.id">{{ report.accountId }}</span></span>
            </div>
            <ZButton size="small" :disabled="busy" @click="openReport(report.id)"><template #icon><RefreshCw :size="14" /></template>刷新报告</ZButton>
          </div>
          <div class="rpt-verdict" :class="`tone-${verdictTone(shownVerdict(report))}`">
            <div class="rpt-verdict-main">
              <span class="rpt-caption">检测结论</span>
              <span class="pill dot lg" :class="[`tone-${verdictTone(shownVerdict(report))}`, { pulse: isActive(report.status) }]">{{ verdict(shownVerdict(report)) }}</span>
            </div>
            <div v-if="reportTop" class="rpt-metric">
              <span class="rpt-caption">最高候选</span>
              <strong class="mono" :class="{ 'text-danger': reportDegraded.has(report.rounds.at(-1)!.score.prediction) }">{{ reportTop.label }}</strong>
            </div>
            <div v-if="reportTop" class="rpt-metric">
              <span class="rpt-caption">概率</span>
              <strong class="tabular">{{ percent(reportTop.probability) }}</strong>
            </div>
            <div v-if="detailText(report)" class="rpt-status" :class="detailClass(report)" role="status" aria-live="polite">{{ detailText(report) }}</div>
          </div>
          <dl class="rpt-facts">
            <div><dt>请求模型</dt><dd class="mono" :title="report.config.requestModel">{{ report.config.requestModel }}</dd></div>
            <div><dt>检测轮次</dt><dd class="tabular">{{ report.rounds.length }} 轮</dd></div>
            <div><dt>采样尝试</dt><dd class="tabular">{{ report.attempts.length }} 次</dd></div>
            <div><dt>指纹库</dt><dd class="mono" :title="report.bankHash">{{ report.bankCommit?.slice(0,12) || '—' }}</dd></div>
            <div><dt>{{ report.finishedAt ? '完成时间' : '创建时间' }}</dt><dd class="tabular" :title="date(report.finishedAt || report.createdAt)">{{ when(report.finishedAt || report.createdAt) }}</dd></div>
          </dl>
        </section>

        <section v-if="report.trigger" class="rpt-trigger" aria-label="触发来源">
          <div class="rpt-trigger-head"><Zap :size="15" aria-hidden="true" /><b>触发来源</b><span class="pill tone-warning">HTTP {{ report.trigger.code }}</span><span class="rpt-trigger-time" :title="date(report.trigger.observedAt)">{{ when(report.trigger.observedAt) }}观察到</span></div>
          <dl class="rpt-kv">
            <div><dt>请求 ID</dt><dd class="mono selectable">{{ report.trigger.requestId || '—' }}</dd></div>
            <div v-if="report.trigger.message"><dt>错误信息</dt><dd>{{ report.trigger.message }}</dd></div>
          </dl>
        </section>

        <div v-if="report.disableState==='failed'" class="notice tone-danger rpt-action"><CircleAlert :size="16" aria-hidden="true" /><span>关闭调度操作失败：{{ report.disableError || '请检查宿主状态' }}</span><ZButton size="small" :disabled="busy" @click="retryDisable(report.id)"><template #icon><RotateCcw :size="13" /></template>重试关闭调度</ZButton></div>
        <div v-if="report.groupState==='failed'" class="notice tone-danger rpt-action"><CircleAlert :size="16" aria-hidden="true" /><span>移组操作失败：{{ report.groupError || '请检查宿主状态' }}</span><ZButton size="small" :disabled="busy" @click="retryGroup(report.id)"><template #icon><RotateCcw :size="13" /></template>重试移入分组</ZButton></div>

        <section class="rpt-section">
          <div class="rpt-section-head">
            <h3>模型概率</h3>
            <span v-if="currentRound" class="rpt-families"><span v-for="f in currentRound.score.families" :key="f.family" class="pill tone-neutral">{{ f.display_name }} <b class="tabular">{{ percent(f.probability) }}</b></span></span>
            <ZSelect v-if="report.rounds.length" v-model="roundIndex" class="rpt-round" :options="report.rounds.map((r,i)=>({value:i,label:`第${r.number}轮 · ${verdict(r.verdict)} · ${date(r.finishedAt)}`}))" aria-label="报告轮次" />
          </div>
          <template v-if="currentRound">
            <div class="prob-list">
              <div v-for="(r,i) in shownResults" :key="r.model" class="prob-row" :class="{ top: i===0, degraded: reportDegraded.has(r.model) }">
                <span class="prob-name"><span class="mono" :title="r.model">{{ r.label }}</span><span v-if="reportDegraded.has(r.model)" class="pill sm tone-danger">降智模型</span></span>
                <span class="prob-bar" aria-hidden="true"><i :style="{ width: `${r.probability*100}%` }" /><s v-if="reportDegraded.has(r.model) && thresholdShown" :style="{ left: `${report.config.degradedPercent}%` }" /></span>
                <b class="prob-value tabular">{{ percent(r.probability) }}</b>
              </div>
            </div>
            <div class="prob-foot">
              <span class="help">标红为配置的降智模型<template v-if="thresholdShown">，竖线为降智判定阈值 {{ report.config.degradedPercent }}%</template>。</span>
              <button v-if="hiddenModels > 0" type="button" class="link-button" :aria-expanded="showAllModels" @click="showAllModels=!showAllModels">{{ showAllModels ? '收起' : `显示全部 ${sortedResults.length} 个模型` }}</button>
            </div>
          </template>
          <div v-else class="rpt-empty">尚无完整轮次结果。</div>
        </section>

        <div class="rpt-folds">
          <details class="rpt-fold">
            <summary><ChevronRight :size="15" class="fold-caret" aria-hidden="true" /><ScrollText :size="15" aria-hidden="true" /><span>配置快照与版本</span><span class="fold-meta mono">{{ report.bankCommit?.slice(0,7) }}</span></summary>
            <div class="fold-body">
              <dl class="rpt-kv grid">
                <div v-for="[k,v] in snapshot" :key="k"><dt>{{ k }}</dt><dd :class="{ mono: /模型|提交/.test(k) }">{{ v }}</dd></div>
              </dl>
              <details class="rpt-json"><summary>原始 JSON</summary><pre>{{ JSON.stringify({config:report.config,bankHash:report.bankHash,bankCommit:report.bankCommit,engineCommit:report.engineCommit},null,2) }}</pre></details>
            </div>
          </details>
          <details class="rpt-fold">
            <summary><ChevronRight :size="15" class="fold-caret" aria-hidden="true" /><ListChecks :size="15" aria-hidden="true" /><span>请求记录（{{ report.attempts.length }}）</span><span v-if="report.attempts.some(a=>a.error)" class="fold-meta text-danger">失败 {{ report.attempts.filter(a=>a.error).length }} 次</span></summary>
            <ol class="attempt-list fold-body">
              <li v-for="(a,i) in report.attempts" :key="i" class="attempt" :class="{ failed: !!a.error }">
                <span class="attempt-icon"><CircleAlert v-if="a.error" :size="15" /><CircleCheck v-else :size="15" /></span>
                <div class="attempt-main">
                  <div class="attempt-line"><span class="attempt-result">{{ a.error || (a.diagnostics?.partial ? '已接纳部分结果（样本校验通过）' : '有效样本') }}</span><time class="tabular">{{ date(a.at) }}</time></div>
                  <div class="attempt-meta"><code class="mono selectable">{{ a.requestId || '未取得请求ID' }}</code><span v-if="a.diagnostics">{{ diagText(a.diagnostics) }}</span></div>
                </div>
              </li>
            </ol>
          </details>
        </div>
      </div>
    </ZDialog>
  </main>
</template>
