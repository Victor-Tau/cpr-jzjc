// Run after installing Playwright in .cache/browser and generating .cache/ui-fixture.json.
import { chromium } from '../.cache/browser/node_modules/playwright/index.mjs'
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { resolve, extname } from 'node:path'
import { resolveTheme, DEFAULT_THEME_COLOR, DEFAULT_CUSTOM_THEME_COLOR } from '../frontend/node_modules/@codex-proxy/ui/dist/theme/index.js'
const root=resolve(import.meta.dirname,'..')
const fixture=JSON.parse(await readFile(resolve(root,'.cache/ui-fixture.json'),'utf8'))
fixture.themes=Object.fromEntries(['light','dark'].map(name=>[name,resolveTheme(name,DEFAULT_THEME_COLOR,DEFAULT_CUSTOM_THEME_COLOR).tokens]))
const browser=await chromium.launch({headless:true,args:['--no-sandbox']})
try {
  const page=await browser.newPage({viewport:{width:1440,height:1050}})
  const errors=[];page.on('pageerror',e=>errors.push(e.message))
  await page.addInitScript(f=>{
    f.revision=1
    window.__calls=[];window.__fixture=f
    // RS plugin pages may run outside a secure context where randomUUID is absent.
    try { Object.defineProperty(globalThis.crypto, 'randomUUID', { configurable:true, value:undefined }) } catch {}
    window.__randomUuidFallback = typeof globalThis.crypto?.randomUUID !== 'function'
    document.addEventListener('DOMContentLoaded',()=>{for(const [k,v] of Object.entries(f.themes.light))document.documentElement.style.setProperty(k,v)})
    window.codexProxyPlugin={version:2,theme:'light',async request(input){
      window.__calls.push(input)
      const b=f.bootstrap;let result={ok:true}
      if(input.path==='api/bootstrap')result=b
      else if(input.path==='api/accounts')result={accounts:f.accounts,nextOffset:null}
      else if(input.path==='api/progress'){const revision='ui:'+f.revision,changed=new URLSearchParams(input.query).get('since')!==revision;result={revision,changed,jobs:changed?b.jobs:null,nextDue:b.nextDue,scheduleError:null}}
      else if(input.path==='api/jobs')result={jobs:b.jobs,banks:b.banks,nextDue:b.nextDue,scheduleError:null}
      else if(input.path==='api/report')result=f.report
      else if(input.path==='api/models')result={models:['gpt-6.1-sol','gpt-5.6-luna']}
      else if(input.path==='api/config'){const data=JSON.parse(input.body);b.config=data.config;b.configRevision++;result={config:data.config,revision:b.configRevision}}
      else if(input.path==='api/submit')result={submitted:['ui-test']}
      else if(input.path==='api/banks/check'){b.banks.update.available={...b.banks.active,commit:'b'.repeat(40),hash:'c'.repeat(64)};b.banks.update.added=['new-model'];b.banks.update.checkedAt=Date.now();result=b.banks}
      else if(input.path==='api/banks/apply'){b.banks.previous=b.banks.active;b.banks.active=b.banks.update.available;b.banks.update.available=null;result=b.banks}
      else if(input.path==='api/banks/rollback'){[b.banks.active,b.banks.previous]=[b.banks.previous,b.banks.active];result=b.banks}
      return {status:200,contentType:'application/json',body:new TextEncoder().encode(JSON.stringify(result)).buffer}
    }}
  },fixture)
  await page.route('http://plugin.test/**',async route=>{
    try {const url=new URL(route.request().url()); const file=resolve(root,'frontend/dist',url.pathname==='/'?'index.html':'.'+url.pathname);assert(file.startsWith(resolve(root,'frontend/dist')+'/'));const body=await readFile(file);await route.fulfill({body,contentType:({'.html':'text/html','.js':'text/javascript','.css':'text/css'})[extname(file)] || 'application/octet-stream'})}catch{await route.fulfill({status:404,body:''})}
  })
  await page.goto('http://plugin.test/')
  await page.getByRole('textbox',{name:'搜索账户',exact:true}).waitFor()
  // The complete visual checkbox must fit inside its cell and remain clickable.
  const assertSelectionVisible = async () => {
    const rows = await page.locator('tbody tr').evaluateAll(rows => rows.map(row => {
      const cell=row.querySelector('td'), label=cell?.querySelector('label'), box=label?.querySelector('span')
      if(!cell || !label || !box)return false
      const c=cell.getBoundingClientRect(), b=box.getBoundingClientRect()
      return b.width>=16 && b.height>=16 && b.left>=c.left && b.right<=c.right && b.top>=c.top && b.bottom<=c.bottom
        && (b.bottom>innerHeight || [b.left+1,b.right-1].every(x=>label.contains(document.elementFromPoint(x,b.top+b.height/2))))
    }))
    assert(rows.length>0 && rows.every(Boolean),'account selection checkbox is clipped or covered')
  }
  await assertSelectionVisible()
  for(const [label, tone] of [['正常','success'],['异常','danger'],['额度耗尽','warning'],['限流','warning'],['已停用','muted']]) {
    const badge=page.locator('.account-status').filter({hasText:label}).first()
    assert.match(await badge.getAttribute('class'),new RegExp(`\\b${tone}\\b`))
  }
  await page.getByRole('textbox',{name:'搜索账户',exact:true}).fill('user@example.com')
  await page.getByText('匹配 1 个',{exact:false}).waitFor()
  await page.getByLabel('选择 user@example.com',{exact:true}).locator('..').click()
  await page.getByRole('button',{name:'检测所选（1）'}).click()
  await page.getByRole('button',{name:'加入检测队列',exact:true}).click()
  await page.getByText('检测已加入后台队列，离开页面后仍会继续。').waitFor()
  assert.equal(await page.evaluate(()=>window.__randomUuidFallback),true)
  await page.getByRole('textbox',{name:'搜索账户',exact:true}).fill('')
  // Exercise a real dropdown, not only the text search.
  await page.getByLabel('订阅类型筛选',{exact:true}).click()
  await page.getByRole('option',{name:'pro',exact:true}).click()
  await page.getByRole('listbox').waitFor({state:'hidden'})
  await page.getByText('匹配 11 个',{exact:false}).waitFor()
  await page.screenshot({path:resolve(root,'.cache/accounts.png'),fullPage:true})
  await page.getByRole('button',{name:'参数配置',exact:true}).click()
  assert.equal(await page.getByRole('button',{name:'保存全部参数',exact:true}).count(),1)
  const timeout=page.getByRole('spinbutton',{name:/^单次请求超时（秒）/})
  assert.equal(await timeout.inputValue(),'120')
  assert.equal(await timeout.getAttribute('max'),null)
  await timeout.fill('3600')
  await page.getByRole('textbox',{name:'请求模型',exact:true}).fill('custom-request-model')
  await page.getByRole('button',{name:'保存全部参数',exact:true}).click()
  await page.getByText('参数已保存。',{exact:true}).waitFor()
  assert.equal(await page.evaluate(()=>window.__fixture.bootstrap.config.requestModel),'custom-request-model')
  assert.equal(await page.evaluate(()=>window.__fixture.bootstrap.config.timeoutSeconds),3600)
  await timeout.fill('0')
  await page.getByRole('button',{name:'保存全部参数',exact:true}).click()
  await page.waitForFunction(()=>window.__fixture.bootstrap.config.timeoutSeconds===0)
  await timeout.fill('120')
  await page.getByRole('button',{name:'保存全部参数',exact:true}).click()
  await page.waitForFunction(()=>window.__fixture.bootstrap.config.timeoutSeconds===120)
  await page.getByRole('button',{name:'读取',exact:true}).click()
  await page.getByRole('button',{name:'检查更新',exact:true}).click()
  await page.getByRole('button',{name:'下载并应用',exact:true}).click()
  assert.equal(await page.evaluate(()=>window.__fixture.bootstrap.banks.active.commit),'b'.repeat(40))
  await page.getByRole('button',{name:'回退上一版本',exact:true}).click()
  await page.screenshot({path:resolve(root,'.cache/settings.png'),fullPage:true})
  await page.getByRole('button',{name:'检测历史',exact:true}).click()
  await page.getByRole('button',{name:'报告',exact:true}).first().click()
  await page.getByText('配置快照与版本',{exact:true}).waitFor()
  await page.screenshot({path:resolve(root,'.cache/report.png'),fullPage:true})
  await page.keyboard.press('Escape')
  await page.setViewportSize({width:430,height:932})
  await page.getByRole('button',{name:'参数配置',exact:true}).click()
  await page.evaluate(()=>{for(const [k,v] of Object.entries(window.__fixture.themes.dark))document.documentElement.style.setProperty(k,v);window.codexProxyPlugin.theme='dark';window.dispatchEvent(new Event('codex-proxy-themechange'))})
  assert.equal(await page.getByRole('button',{name:'保存全部参数',exact:true}).count(),1)
  await page.screenshot({path:resolve(root,'.cache/mobile.png'),fullPage:true})
  await page.getByRole('button',{name:'账户检测',exact:true}).click()
  await assertSelectionVisible()
  // A live task must update the list and an already open report without clicks.
  await page.setViewportSize({width:1440,height:1050})
  await page.getByLabel('订阅类型筛选',{exact:true}).click()
  await page.getByRole('option',{name:'全部订阅',exact:true}).click()
  await page.getByRole('textbox',{name:'搜索账户',exact:true}).fill('user@example.com')
  await page.evaluate(()=>{
    const f=window.__fixture,j=f.bootstrap.jobs[0]
    j.status=f.report.status='running'
    j.progress=f.report.progress={stage:'receiving',stageStartedAt:Date.now()-2000,updatedAt:Date.now(),detail:{receivedBytes:2048}}
    f.revision++;document.dispatchEvent(new Event('visibilitychange'))
  })
  await page.locator('.live-progress').filter({hasText:'正在接收回答'}).waitFor({timeout:2500})
  assert.match(await page.locator('.live-progress').innerText(),/2\.0 KB/)
  await page.getByRole('button',{name:'报告',exact:true}).click()
  await page.getByText('配置快照与版本',{exact:true}).waitFor()
  await page.evaluate(()=>{
    const f=window.__fixture
    f.bootstrap.jobs[0].progress=f.report.progress={stage:'scoring',stageStartedAt:Date.now(),updatedAt:Date.now(),detail:{}}
    f.revision++
  })
  await page.getByRole('status').filter({hasText:'正在分析结果'}).last().waitFor({timeout:2500})
  const beforeReportCalls=await page.evaluate(()=>window.__calls.filter(c=>c.path==='api/report').length)
  await page.evaluate(()=>{
    const f=window.__fixture,j=f.bootstrap.jobs[0]
    f.report.rounds.push({...f.report.rounds[0],number:2})
    j.roundCount=2;j.status=f.report.status='completed';j.message=f.report.message='本轮检测已自动完成'
    delete j.progress;delete f.report.progress;f.revision++
  })
  await page.getByRole('status').filter({hasText:'本轮检测已自动完成'}).last().waitFor({timeout:2500})
  assert(await page.evaluate(()=>window.__calls.filter(c=>c.path==='api/report').length)>beforeReportCalls)
  assert.match(await page.getByLabel('报告轮次',{exact:true}).innerText(),/第2轮/)
  await page.keyboard.press('Escape')
  await page.evaluate(()=>{Object.defineProperty(document,'hidden',{configurable:true,value:true});document.dispatchEvent(new Event('visibilitychange'))})
  const paused=await page.evaluate(()=>window.__calls.filter(c=>c.path==='api/progress').length)
  await page.waitForTimeout(1200)
  assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.path==='api/progress').length),paused)
  await page.evaluate(()=>{Object.defineProperty(document,'hidden',{configurable:true,value:false});document.dispatchEvent(new Event('visibilitychange'))})
  await page.waitForFunction(count=>window.__calls.filter(c=>c.path==='api/progress').length>count,paused,{timeout:1500})
  assert.deepEqual(errors,[])
  const paths=await page.evaluate(()=>window.__calls.map(c=>c.path))
  for(const path of ['api/submit','api/models','api/config','api/banks/check','api/banks/apply','api/banks/rollback','api/report'])assert(paths.includes(path),path)
  console.log(JSON.stringify({ui:'passed',checks:['unclipped checkboxes on desktop and mobile','colored account statuses','single save button','search','subscription filter','batch submit','configuration save','model catalog','bank update and rollback','report','mobile and dark mode','live progress within 2.5 seconds','automatic report completion','pause and resume on visibility change'],pageErrors:errors},null,2))
} finally {await browser.close()}
