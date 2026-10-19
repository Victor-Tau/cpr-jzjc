<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Search, RefreshCw, Play, Square, Settings2, Users, History, X } from '@lucide/vue'
import { ZButton } from '@codex-proxy/ui/button'
import { ZInput } from '@codex-proxy/ui/input'
import { ZSelect } from '@codex-proxy/ui/select'
import { ZCheckbox } from '@codex-proxy/ui/checkbox'
import { ZTable, type TableColumn } from '@codex-proxy/ui/table'
import { ZPagination } from '@codex-proxy/ui/pagination'
import { ZTag } from '@codex-proxy/ui/tag'
import { ZDialog } from '@codex-proxy/ui/dialog'
import Settings from './components/Settings.vue'
import { api, active, date, percent, status, verdict } from './api'
import { createRequestId } from './request-id'
import type { Account, Banks, Config, Group, Job, Report } from './types'

const tab = ref('accounts'), loading = ref(true), busy = ref(false), error = ref(''), notice = ref('')
const accounts = ref<Account[]>([]), jobs = ref<Job[]>([]), config = ref<Config>(), banks = ref<Banks>()
const allGroups = ref<Group[]>([])
const keys = ref<{ id:string; name:string }[]>([]), revision = ref(0), settingsKey = ref(0), nextDue = ref<number|null>(null), scheduleError = ref<string|null>(null)
const search = ref(''), plan = ref(''), state = ref(''), group = ref(''), resultFilter = ref(''), page = ref(1), pageSize = ref(20), selected = ref<string[]>([])
const historySearch = ref(''), historyVerdict = ref(''), historyPage = ref(1), historySize = ref(20)
const report = ref<Report>(), reportOpen = ref(false), roundIndex = ref(0)
const confirmIds = ref<string[]>([]), confirmOpen = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined, clockTimer: ReturnType<typeof setInterval> | undefined, polling = false, disposed = false, failures = 0, progressRevision: string | undefined
const clock = ref(Date.now())
const requestIds = new Map<string,string>()
const plans = computed(() => [{ value:'',label:'全部订阅' }, ...Array.from(new Set(accounts.value.map(a => a.planType || 'unknown'))).sort().map(p => ({ value:p,label:p }))])
const groups = computed(() => [{ value:'',label:'全部分组' }, {value:'__ungrouped__',label:'未分组'}, ...Array.from(new Map(accounts.value.flatMap(a => a.groups).map(g => [g.id,g])).values()).map(g => ({value:g.id,label:g.name}))])
const statuses = computed(() => [{value:'',label:'全部账户状态'},{value:'disabled',label:'已停用'},...Array.from(new Set(accounts.value.map(a => a.status))).map(s => ({ value:s,label:({normal:'正常',error:'异常',quota_exhausted:'额度耗尽',rate_limited:'限流'} as Record<string,string>)[s] || s }))])
const accountStatusClass = (a: Account) => !a.enabled ? 'muted' : ({ normal:'success', error:'danger', quota_exhausted:'warning', rate_limited:'warning' } as Record<string,string>)[a.status] || 'info'
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
    && (!resultFilter.value || (resultFilter.value === 'active' ? jobs.value.some(j => j.accountId === a.id && active(j)) : j?.verdict === resultFilter.value))
}))
const visible = computed(() => filtered.value.slice((page.value-1)*pageSize.value,page.value*pageSize.value))
const pageCandidates = computed(() => visible.value.filter(canDetect))
const chosen = computed(() => accounts.value.filter(a => selected.value.includes(a.id) && canDetect(a)))
const allPage = computed(() => pageCandidates.value.length > 0 && pageCandidates.value.every(a => selected.value.includes(a.id)))
const activeJobs = computed(() => jobs.value.filter(j => active(j)).length)
const manualJobs = computed(() => jobs.value.filter(j => j.source === 'manual' && active(j)).length)
const scheduledJobs = computed(() => jobs.value.filter(j => j.source === 'schedule' && active(j)).length)
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
  if (!s?.enabled) return '自动定时检测已关闭，可在参数配置中开启'
  return s.kind === 'daily' ? `自动定时检测：每日 ${s.time}（${s.timezone}）` : `自动定时检测：每 ${s.intervalMinutes} 分钟`
})
function shortWhen(value: number) {
  const at = new Date(value), now = new Date(clock.value)
  const time = at.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false })
  if (at.toDateString() === now.toDateString()) return time
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)
  if (at.toDateString() === tomorrow.toDateString()) return `明天 ${time}`
  return `${at.getMonth() + 1}月${at.getDate()}日 ${time}`
}
const nextRun = computed(() => config.value?.schedule.enabled ? nextDue.value : null)
const runWhen = computed(() => nextRun.value == null ? '—' : nextRun.value <= clock.value + 2000 ? '现在' : shortWhen(nextRun.value))
const degradedCount = computed(() => Array.from(latest.value.values()).filter(j => j.verdict === 'degraded').length)
const history = computed(() => jobs.value.filter(j => (!historySearch.value || `${j.accountName} ${j.accountId}`.toLowerCase().includes(historySearch.value.toLowerCase())) && (!historyVerdict.value || j.verdict === historyVerdict.value || j.status === historyVerdict.value)))
const visibleHistory = computed(() => history.value.slice((historyPage.value-1)*historySize.value,historyPage.value*historySize.value))
const currentRound = computed(() => report.value?.rounds[roundIndex.value])
watch([search,plan,state,group,resultFilter,pageSize], () => { page.value=1 })
watch([historySearch,historyVerdict,historySize], () => { historyPage.value=1 })
watch(filtered, a => { page.value=Math.min(page.value,Math.max(1,Math.ceil(a.length/pageSize.value))) })

const columns: TableColumn<Account>[] = [{key:'select',label:'',kind:'selection',width:48},{key:'identity',label:'账户',kind:'identity',minWidth:240},{key:'plan',label:'订阅',width:95},{key:'groups',label:'分组',minWidth:135},{key:'status',label:'账户状态',width:110},{key:'result',label:'检测结果',minWidth:240},{key:'time',label:'检测时间',minWidth:235},{key:'actions',label:'操作',kind:'actions',width:150}]
const historyColumns: TableColumn<Job>[] = [{key:'account',label:'账户',kind:'identity',minWidth:220},{key:'result',label:'结论',minWidth:150},{key:'model',label:'最高候选',minWidth:160},{key:'probability',label:'概率',width:85},{key:'createdAt',label:'检测时间',minWidth:235},{key:'actions',label:'操作',kind:'actions',width:170}]

function selectOne(id:string,checked:boolean) { selected.value=checked ? Array.from(new Set([...selected.value,id])) : selected.value.filter(x => x !== id) }
function selectPage(checked:boolean) { const ids=pageCandidates.value.map(a => a.id); selected.value=checked ? Array.from(new Set([...selected.value,...ids])) : selected.value.filter(id => !ids.includes(id)) }
async function refreshJobs() { const r=await api.jobs(); jobs.value=r.jobs; banks.value=r.banks; nextDue.value=r.nextDue; scheduleError.value=r.scheduleError; progressRevision=undefined }
function progressText(j: Job | Report) {
  const p=j.progress
  if (!p || !active(j as Job)) return j.message
  const labels: Record<string,string>={preparing:'正在准备挑战',requesting:'正在发送请求',waiting_response:'等待模型响应',receiving:'正在接收回答',validating:'正在校验样本',waiting_score:'已获得有效样本，等待评分',scoring:'正在分析结果',waiting_retry:'等待重试',waiting_retest:'等待复测',cancelling:'正在取消检测',waiting_request:'准备完成，等待发起请求'}
  const remaining=p.detail.nextAt ? Math.max(0,Math.ceil((p.detail.nextAt-clock.value)/1000)) : 0
  if (p.stage==='waiting_retry' || p.stage==='waiting_retest') return `${j.message || labels[p.stage]}${j.roundAttempts ? `（本轮已尝试 ${j.roundAttempts}/5 次）` : ''} · ${remaining>0 ? `${remaining} 秒后可执行` : '等待调度'}`
  const elapsed=Math.max(0,Math.floor((clock.value-p.stageStartedAt)/1000))
  const d=p.detail
  const traffic=d.outputBytes != null ? ` · 正文 ${(d.outputBytes/1024).toFixed(1)} KB · 推理 ${((d.reasoningBytes ?? 0)/1024).toFixed(1)} KB` : d.receivedBytes ? ` · 流数据 ${(d.receivedBytes/1024).toFixed(1)} KB` : ''
  return `${labels[p.stage] || j.message} · ${elapsed} 秒${traffic}`
}
async function refreshProgress() {
  const r=await api.progress(progressRevision)
  if (disposed) return
  progressRevision=r.revision; nextDue.value=r.nextDue; scheduleError.value=r.scheduleError
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
async function load() { loading.value=true; try { const [b,a]=await Promise.all([api.bootstrap(),api.accounts()]); config.value=b.config; if(!b.configRevision) config.value.schedule.timezone=Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'; revision.value=b.configRevision; banks.value=b.banks; keys.value=b.keys; allGroups.value=b.groups; jobs.value=b.jobs; accounts.value=a.accounts; nextDue.value=b.nextDue; scheduleError.value=b.scheduleError; settingsKey.value++ } catch(e) { error.value=(e as Error).message } finally { loading.value=false } }
function openSubmit(ids:string[]) { confirmIds.value=ids; confirmOpen.value=true }
async function submit() { await action(async () => { const signature=[...confirmIds.value].sort().join(','); let id=requestIds.get(signature); if(!id) { id=createRequestId();requestIds.set(signature,id) } await api.submit(confirmIds.value,id); requestIds.delete(signature); confirmOpen.value=false; selected.value=selected.value.filter(id => !confirmIds.value.includes(id)); await refreshJobs(); notice.value='检测已加入后台队列，离开页面后仍会继续。' }) }
async function save(c:Config) { await action(async () => { const r=await api.save(c,revision.value); config.value=r.config; revision.value=r.revision; settingsKey.value++; await refreshJobs(); notice.value='参数已保存。' }) }
async function bank(operation:'check'|'apply'|'rollback'|'builtin') { await action(async () => { banks.value=await api.bank(operation); notice.value=operation==='check' ? (banks.value.update.error ? '' : banks.value.update.available ? '发现可用更新。' : '当前指纹库已是最新内容。') : '指纹库版本已切换，新任务将使用此版本。' }) }
async function openReport(id:string) { await action(async () => { report.value=await api.report(id); roundIndex.value=0; reportOpen.value=true }) }
async function cancel(id?:string, source?:'manual'|'schedule') { await action(async () => { await api.cancel(id,source); await refreshJobs() }) }
async function remove(id:string) { await action(async () => { await api.delete(id); await refreshJobs() }) }
async function retryDisable(id:string) { await action(async () => { await api.retryDisable(id); await refreshJobs(); await loadAccounts(); if(report.value?.id===id) report.value=await api.report(id) }) }
async function retryGroup(id:string) { await action(async () => { await api.retryGroup(id); await refreshJobs(); await loadAccounts(); if(report.value?.id===id) report.value=await api.report(id) }) }
onMounted(async () => { document.addEventListener('visibilitychange',visibilityChanged); clockTimer=setInterval(()=>{clock.value=Date.now()},1000); await load(); schedulePoll(0) })
onUnmounted(() => { disposed=true; if(timer) clearTimeout(timer); if(clockTimer) clearInterval(clockTimer); document.removeEventListener('visibilitychange',visibilityChanged) })
</script>

<template>
  <main>
    <div class="topline"><nav aria-label="检测插件导航"><ZButton :type="tab==='accounts'?'primary':'default'" variant="text" @click="tab='accounts'"><template #icon><Users :size="16" /></template>账户检测</ZButton><ZButton :type="tab==='settings'?'primary':'default'" variant="text" @click="tab='settings'"><template #icon><Settings2 :size="16" /></template>参数配置</ZButton><ZButton :type="tab==='history'?'primary':'default'" variant="text" @click="tab='history'"><template #icon><History :size="16" /></template>检测历史</ZButton></nav><span v-if="banks" class="version">指纹库 {{ banks.active.commit.slice(0,7) }} · {{ banks.active.models.length }} 个模型</span></div>
    <div v-if="error" class="message danger" role="alert"><span>{{ error }}</span><button aria-label="关闭错误提示" @click="error=''">×</button></div>
    <div v-if="notice" class="message success" role="status"><span>{{ notice }}</span><button aria-label="关闭提示" @click="notice=''">×</button></div>
    <p v-if="loading">正在读取账户与插件配置…</p>
    <template v-if="tab==='accounts'">
      <div class="counters"><button @click="resultFilter=''">全部账户 <b>{{ accounts.length }}</b></button><button @click="resultFilter='active'">检测队列 <b>{{ activeJobs }}</b></button><button class="danger" @click="resultFilter='degraded'">确认降智 <b>{{ degradedCount }}</b></button><span class="auto-pill" :class="config?.schedule.enabled ? 'on' : 'off'" role="status" :title="autoTitle">{{ config?.schedule.enabled ? `自动检测 · ${autoPeriod}` : '自动检测 · 已关闭' }}</span><span class="next-run" :title="nextRun != null ? `下次自动检测：${date(nextRun)}` : '当前没有待执行的自动检测'">下次检测 <b class="small">{{ runWhen }}</b></span></div>
      <div class="toolbar"><ZInput v-model="search" class="search" placeholder="搜索名称、邮箱或账户ID" clearable aria-label="搜索账户"><template #prefix><Search :size="16" /></template></ZInput><ZSelect v-model="plan" class="filter" :options="plans" aria-label="订阅类型筛选" /><ZSelect v-model="state" class="filter" :options="statuses" aria-label="账户状态筛选" /><ZSelect v-model="group" class="filter" :options="groups" aria-label="账户分组筛选" /><div class="commands"><ZButton :disabled="busy" @click="action(loadAccounts)"><template #icon><RefreshCw :size="15" /></template>刷新账户</ZButton><ZButton v-if="manualJobs" :disabled="busy" @click="cancel(undefined,'manual')"><template #icon><Square :size="14" /></template>停止手动队列</ZButton><ZButton v-if="scheduledJobs" :disabled="busy" @click="cancel(undefined,'schedule')"><template #icon><Square :size="14" /></template>停止自动队列</ZButton><ZButton type="primary" :disabled="busy || !chosen.length" @click="openSubmit(chosen.map(a=>a.id))"><template #icon><Play :size="15" /></template>检测所选（{{ chosen.length }}）</ZButton></div></div>
      <div class="selection"><ZCheckbox :model-value="allPage" :disabled="!pageCandidates.length || busy" label="选择当前页" @change="selectPage" /><span>已选 {{ chosen.length }} 个 · 匹配 {{ filtered.length }} 个</span><ZButton v-if="selected.length" size="small" variant="text" @click="selected=[]">清空选择</ZButton><ZButton v-if="resultFilter" size="small" variant="text" @click="resultFilter=''">清除结果筛选</ZButton></div>
      <ZTable :columns="columns" :data="visible" :row-key="a=>a.id" :loading="loading" empty-text="没有匹配的账户">
        <template #select="{row}"><ZCheckbox :model-value="selected.includes(row.id)" :disabled="!canDetect(row) || busy" :aria-label="`选择 ${row.email || row.name}`" @change="checked=>selectOne(row.id,checked)" /></template>
        <template #identity="{row}"><div class="identity"><span class="avatar">{{ (row.email || row.name || '?').slice(0,1).toUpperCase() }}</span><div><b>{{ row.email || row.name }}</b><small :title="row.id">{{ row.name }} · {{ row.provider }}</small></div></div></template>
        <template #plan="{row}"><ZTag size="small">{{ row.planType || '未知' }}</ZTag></template>
        <template #groups="{row}"><div class="tags"><ZTag v-for="g in row.groups" :key="g.id" size="small">{{ g.name }}</ZTag><span v-if="!row.groups.length" class="muted">未分组</span></div></template>
        <template #status="{row}"><span class="account-status" :class="accountStatusClass(row)">{{ status(row) }}</span></template>
        <template #result="{row}"><div v-if="!row.supported" class="muted">暂不支持此提供方</div><div v-else-if="jobOf(row)" class="result"><b :class="{danger:jobOf(row)?.verdict==='degraded',success:jobOf(row)?.verdict==='no_match'}">{{ verdict(active(jobOf(row)) ? jobOf(row)!.status : jobOf(row)!.status !== 'completed' ? jobOf(row)!.status : jobOf(row)!.verdict) }}</b><small v-if="jobOf(row)?.prediction">{{ jobOf(row)?.prediction }} · {{ percent(jobOf(row)?.probability) }}</small><small class="live-progress" role="status">{{ progressText(jobOf(row)!) }}</small></div><span v-else class="muted">尚未检测</span></template>
        <template #time="{row}"><span class="date">{{ date(jobOf(row)?.finishedAt || jobOf(row)?.createdAt) }}<em v-if="jobOf(row)" class="detection-source" :class="jobOf(row)?.source === 'schedule' ? 'auto' : 'manual'">{{ jobOf(row)?.source === 'schedule' ? '（自动）' : '（手动）' }}</em></span></template>
        <template #actions="{row}"><div class="inline"><ZButton size="small" :disabled="!canDetect(row) || busy" @click="openSubmit([row.id])">检测</ZButton><ZButton v-if="jobOf(row)" size="small" variant="text" :disabled="busy" @click="openReport(jobOf(row)!.id)">报告</ZButton></div></template>
      </ZTable><ZPagination v-model:current-page="page" v-model:page-size="pageSize" :total="filtered.length" :page-sizes="[10,20,50,100]" />
    </template>
    <Settings v-if="tab==='settings' && config && banks" :key="settingsKey" :config="config" :revision="revision" :banks="banks" :keys="keys" :accounts="accounts" :groups="allGroups" :selected="selected" :next-due="nextDue" :schedule-error="scheduleError" :busy="busy" @save="save" @bank="bank" @error="error=$event" />
    <template v-if="tab==='history'">
      <div class="section-heading"><div><h2>检测历史</h2><p>保留各轮概率、参数及指纹库版本。</p></div><ZButton :disabled="busy" @click="action(refreshJobs)">刷新</ZButton></div>
      <div class="toolbar"><ZInput v-model="historySearch" class="search" placeholder="搜索账户" clearable aria-label="搜索历史账户" /><ZSelect v-model="historyVerdict" class="filter" :options="[{value:'',label:'全部结论'},...['degraded','no_match','inconclusive','running','queued','failed','interrupted','cancelled'].map(v=>({value:v,label:verdict(v)}))]" aria-label="历史结论筛选" /></div>
      <ZTable :columns="historyColumns" :data="visibleHistory" :row-key="j=>j.id" empty-text="还没有检测记录">
        <template #account="{row}"><b>{{ row.accountName }}</b></template><template #result="{row}"><span :class="{danger:row.verdict==='degraded'}">{{ verdict(row.status==='completed'?row.verdict:row.status) }}</span></template><template #model="{row}">{{ row.prediction || '—' }}</template><template #probability="{row}">{{ percent(row.probability) }}</template><template #createdAt="{row}"><span class="date">{{ date(row.createdAt) }}<em class="detection-source" :class="row.source === 'schedule' ? 'auto' : 'manual'">{{ row.source === 'schedule' ? '（自动）' : '（手动）' }}</em></span></template><template #actions="{row}"><div class="inline"><ZButton size="small" variant="text" :disabled="busy" @click="openReport(row.id)">报告</ZButton><ZButton v-if="active(row)" size="small" variant="text" :disabled="busy" @click="cancel(row.id)">取消</ZButton><ZButton v-else size="small" variant="text" :disabled="busy" @click="remove(row.id)">删除</ZButton></div></template>
      </ZTable><ZPagination v-model:current-page="historyPage" v-model:page-size="historySize" :total="history.length" :page-sizes="[10,20,50,100]" />
    </template>
    <ZDialog v-model="confirmOpen" title="开始账户检测" width="32rem" :close-on-click-modal="!busy"><p>将 {{ confirmIds.length }} 个账户加入检测队列，请求模型为 <b>{{ config?.requestModel }}</b>，使用所选计量 Key。</p><p>每轮只检测一次，失败时最多尝试五次；确认与低置信度复测会追加轮次。</p><p v-if="config?.autoDisable" class="danger">已开启：确认降智后自动关闭该账户调度。</p><p v-if="config?.autoMoveGroup" class="danger">已开启：确认降智后移入 {{ allGroups.find(g=>g.id===config?.targetGroupId)?.name || config?.targetGroupId }}，替换原有分组。</p><p v-if="error" class="danger">{{ error }}</p><template #footer><ZButton :disabled="busy" @click="confirmOpen=false">取消</ZButton><ZButton type="primary" :loading="busy" @click="submit">加入检测队列</ZButton></template></ZDialog>
    <ZDialog v-model="reportOpen" title="检测报告" width="60rem">
      <template v-if="report"><div class="section-heading"><div><h3>{{ report.accountName }}</h3><p :class="{danger:report.verdict==='degraded'}" role="status" aria-live="polite">{{ progressText(report) }}</p></div><ZButton :disabled="busy" @click="openReport(report.id)">刷新报告</ZButton></div><div class="report-meta"><span>请求模型 <b>{{ report.config.requestModel }}</b></span><span>指纹库 <code>{{ report.bankCommit?.slice(0,12) }}</code></span><span>采样尝试 {{ report.attempts.length }}</span></div><p v-if="report.disableState==='failed'" class="danger">关闭调度操作失败：{{ report.disableError || '请检查宿主状态' }}。<ZButton size="small" :disabled="busy" @click="retryDisable(report.id)">重试关闭调度</ZButton></p><p v-if="report.groupState==='failed'" class="danger">移组操作失败：{{ report.groupError || '请检查宿主状态' }}。<ZButton size="small" :disabled="busy" @click="retryGroup(report.id)">重试移入分组</ZButton></p><ZSelect v-if="report.rounds.length" v-model="roundIndex" :options="report.rounds.map((r,i)=>({value:i,label:`第${r.number}轮 · ${verdict(r.verdict)} · ${date(r.finishedAt)}`}))" aria-label="报告轮次" />
      <template v-if="currentRound"><div class="family-summary"><ZTag v-for="f in currentRound.score.families" :key="f.family">{{ f.display_name }} {{ percent(f.probability) }}</ZTag></div><div class="probabilities"><div v-for="r in currentRound.score.results" :key="r.model" class="probability-row"><span>{{ r.label }}</span><div class="bar"><i :style="{width:`${r.probability*100}%`}" /></div><b>{{ percent(r.probability) }}</b></div></div></template><p v-else class="muted">尚无完整轮次结果。</p>
      <details><summary>配置快照与版本</summary><pre>{{ JSON.stringify({config:report.config,bankHash:report.bankHash,bankCommit:report.bankCommit,engineCommit:report.engineCommit},null,2) }}</pre></details><details><summary>请求记录（{{ report.attempts.length }}）</summary><ul class="attempts"><li v-for="(a,i) in report.attempts" :key="i"><time>{{ date(a.at) }}</time><code>{{ a.requestId || '未取得请求ID' }}</code><span>{{ a.error || (a.diagnostics?.partial ? '已接纳部分结果（样本校验通过）' : '有效样本') }}</span><small v-if="a.diagnostics">耗时 {{ (a.diagnostics.elapsedMs/1000).toFixed(1) }} 秒<template v-if="a.diagnostics.outputBytes != null"> · 正文 {{ (a.diagnostics.outputBytes/1024).toFixed(1) }} KB · 推理 {{ ((a.diagnostics.reasoningBytes ?? 0)/1024).toFixed(1) }} KB</template> · 流数据 {{ (a.diagnostics.receivedBytes/1024).toFixed(1) }} KB<span v-if="a.diagnostics.budgetMs != null"> · 本次请求窗口 {{ (a.diagnostics.budgetMs/1000).toFixed(1) }} 秒</span></small></li></ul></details></template>
    </ZDialog>
  </main>
</template>
