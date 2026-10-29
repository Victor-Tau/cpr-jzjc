// Node 24 直接运行 TypeScript，验证页面实际使用的请求和分页合并逻辑。
import assert from 'node:assert/strict'
import { request } from '../frontend/src/api.ts'

const calls = []
const response = (value, status = 200) => ({ status, body: new TextEncoder().encode(JSON.stringify(value)).buffer })
const install = handler => {
  calls.length = 0
  globalThis.window = { codexProxyPlugin: { version: 2, request: async input => { calls.push(input); return handler(input) } } }
}

for (const field of ['jobs', 'accounts', 'models']) {
  install(input => {
    const params = new URLSearchParams(input.query)
    assert.equal(params.get('since'), 'epoch:2')
    const offset = Number(params.get('pageOffset') || 0)
    if (offset) assert.equal(params.get('pageSnapshot'), 'stable')
    return response({ [field]: offset ? ['第三条'] : ['第一条', '第二条'], revision: 'epoch:3', nextOffset: null,
      pagination: { field, snapshot: 'stable', nextOffset: offset ? null : 2 } })
  })
  assert.deepEqual(await request('GET', 'api/test?since=epoch%3A2'), {
    [field]: ['第一条', '第二条', '第三条'], revision: 'epoch:3', nextOffset: null,
  })
  assert.equal(calls.length, 2)
}

let changed = false
install(input => {
  const offset = new URLSearchParams(input.query).get('pageOffset')
  if (offset && !changed) {
    changed = true
    return response({ error: { code: 'pagination_changed' } }, 409)
  }
  return response({ jobs: [changed ? 'new' : 'old'], pagination: { field: 'jobs', snapshot: changed ? 'new' : 'old', nextOffset: offset ? null : 1 } })
})
assert.deepEqual(await request('GET', 'api/jobs'), { jobs: ['new', 'new'] })
assert.equal(calls.length, 4)

install(() => response({ error: { code: 'pagination_changed' } }, 409))
await assert.rejects(request('GET', 'api/jobs'), /数据正在更新/)
assert.equal(calls.length, 3)

install(() => response({ jobs: [], pagination: { field: 'jobs', snapshot: 'x', nextOffset: 0 } }))
await assert.rejects(request('GET', 'api/jobs'), /分页未前进/)

install(() => response({ error: { message: '配置冲突' } }, 409))
await assert.rejects(request('POST', 'api/config', { revision: 1 }), /配置冲突/)
assert.equal(calls.length, 1)
assert.deepEqual(JSON.parse(calls[0].body), { revision: 1 })
console.log('API pagination: all fields, Unicode, query retention, snapshot restart, retry bound, invalid cursor and POST errors passed.')
