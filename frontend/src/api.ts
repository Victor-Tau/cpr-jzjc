import type { Account, Banks, Config, Group, Job, Report } from './types'

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const host = window.codexProxyPlugin
  if (!host || host.version !== 2) throw new Error('请从 Codex Proxy RS 的插件页面打开')
  const [route, query] = path.split('?')
  const reply = await host.request({ method, path: route!, query, ...(body === undefined ? {} : { contentType: 'application/json', body: JSON.stringify(body) }) })
  let value: any
  try { value = JSON.parse(new TextDecoder().decode(reply.body)) } catch { throw new Error('插件响应格式无效') }
  if (reply.status < 200 || reply.status >= 300) throw new Error(value?.error?.message || `请求失败（HTTP ${reply.status}）`)
  return value as T
}
export interface Bootstrap { config: Config; configRevision: number; groups: Group[]; keys: { id: string; name: string }[]; jobs: Job[]; banks: Banks; nextDue: number | null; scheduleError: string | null }
export const api = {
  bootstrap: () => request<Bootstrap>('GET', 'api/bootstrap'),
  accounts: async () => {
    const accounts: Account[] = []; let offset: number | null = 0
    while (offset !== null) {
      const page: { accounts: Account[]; nextOffset: number | null } = await request('GET', `api/accounts?offset=${offset}`)
      accounts.push(...page.accounts)
      if (page.nextOffset !== null && page.nextOffset <= offset) throw new Error('账户分页未前进，请刷新')
      offset = page.nextOffset
    }
    return { accounts }
  },
  models: (key: string) => request<{ models: string[] }>('GET', `api/models?keyId=${encodeURIComponent(key)}`),
  progress: (since?: string) => request<{ revision: string; changed: boolean; jobs: Job[] | null; nextDue: number | null; scheduleError: string | null }>('GET', `api/progress${since === undefined ? '' : `?since=${encodeURIComponent(since)}`}`),
  jobs: () => request<{ jobs: Job[]; banks: Banks; nextDue: number | null; scheduleError: string | null }>('GET', 'api/jobs'),
  report: (id: string) => request<Report>('GET', `api/report?id=${encodeURIComponent(id)}`),
  save: (config: Config, revision: number) => request<{ revision: number; config: Config }>('POST', 'api/config', { config, revision }),
  previewConfig: (config: unknown) => request<{ config: Config }>('POST', 'api/config/preview', { config }),
  submit: (accountIds: string[], requestId: string) => request('POST', 'api/submit', { accountIds, requestId }),
  cancel: (id?: string) => request('POST', 'api/cancel', { id }),
  delete: (id: string) => request('POST', 'api/delete', { id }),
  retryDisable: (id: string) => request('POST', 'api/retry-disable', { id }),
  retryGroup: (id: string) => request('POST', 'api/retry-group', { id }),
  bank: (operation: 'check' | 'apply' | 'rollback' | 'builtin') => request<Banks>('POST', `api/banks/${operation}`, {}),
}
export const date = (value: number | string | null | undefined) => value ? new Date(value).toLocaleString('zh-CN', { hour12: false }) : '—'
export const percent = (n: number | null | undefined) => n == null ? '—' : `${(n * 100).toFixed(1)}%`
export const active = (job?: Job) => !!job && ['queued', 'running'].includes(job.status)
export const verdict = (s: string) => ({ degraded: '确认降智', suspected: '疑似降智', low_confidence: '低置信度复测', inconclusive: '证据不足', no_match: '未命中降智规则', queued: '排队中', running: '检测中', failed: '检测失败', interrupted: '已中断', cancelled: '已取消', completed: '已完成' }[s] || s || '尚未检测')
export const status = (a: Account) => !a.enabled ? '已停用' : ({ normal: '正常', error: '异常', quota_exhausted: '额度耗尽', rate_limited: '限流' }[a.status] || a.status)
