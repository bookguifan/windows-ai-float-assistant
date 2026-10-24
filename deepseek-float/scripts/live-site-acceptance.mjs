// Opt-in real-site tests. Uses the application's real injection/readback and result UI.
// Start the release with isolated LOCALAPPDATA and CDP first; see docs/live-site-testing.md.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { connect as connectCdp } from './lib/live-cdp.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const args = process.argv.slice(2);
function option(name, fallback) {
  const i = args.indexOf(name);
  return i < 0 ? fallback : args[i + 1];
}
const sites = {chatgpt:'https://chatgpt.com/', gemini:'https://gemini.google.com/app',
  deepseek:'https://chat.deepseek.com/', kimi:'https://www.kimi.com/',
  chatglm:'https://chatglm.cn/main/alltoolsdetail?lang=zh', qianwen:'https://www.qianwen.com/'};
if (args.includes('--help')) {
  console.log('node scripts/live-site-acceptance.mjs --allow-live --site chatgpt --local-dir .test-artifacts/live-20261017/local [--port 9444] [--timeout 90000]');
  process.exit(0);
}
assert(args.includes('--allow-live'), 'Requires --allow-live: sends synthetic test questions to a real AI site.');
const site = option('--site');
assert(sites[site], 'Choose one supported --site per run.');
assert(option('--local-dir'), 'An isolated --local-dir is required.');
const local = fs.realpathSync(path.resolve(root, option('--local-dir')));
const artifacts = fs.realpathSync(path.join(root, '.test-artifacts'));
assert(local.startsWith(artifacts + path.sep), 'LOCALAPPDATA must be inside this project .test-artifacts.');
const store = path.join(local, 'com.deepseekfloat.desktop');
const configPath = path.join(store, 'config.json');
assert(fs.existsSync(configPath), 'Start the isolated test application first.');
const port = Number(option('--port', '9444'));
const timeout = Number(option('--timeout', '90000'));
assert(Number.isInteger(port) && port > 0 && port < 65536);
assert(Number.isFinite(timeout) && timeout >= 1000 && timeout <= 300000);
const run = Date.now().toString(36);
const report = {site, run, started:new Date().toISOString(), mode:'real-webview-release', steps:[]};
const output = path.join(path.dirname(local), `${site}-${run}.json`);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const clients = [];
async function connect(match) {
  const client=await connectCdp(port,match);
  clients.push(client);
  return client;
}
let invoke, original, panel, card;
try {
  const ball=await connect(p=>p.url.endsWith('/poc/ball.html'));
  panel=await connect(p=>p.url.startsWith('https://'));
  card=await connect(p=>p.url.endsWith('/poc/result.html'));
  invoke=(command,a={})=>ball.evaluate(`__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(a)})`);
  original=(await invoke('config_get')).panel_url || sites.deepseek;
  // Prove this running instance writes the supplied isolated directory before any send/favorite.
  const before=fs.statSync(configPath).mtimeMs;
  const proofUrl=original===sites[site]?sites[site]+'#dsf-isolation-'+run:sites[site];
  await invoke('config_set_panel_url',{url:proofUrl});
  assert(fs.statSync(configPath).mtimeMs > before, 'The running application is not using this isolated config.');
  assert.equal(JSON.parse(fs.readFileSync(configPath,'utf8')).panel_url,proofUrl);
  if(proofUrl!==sites[site])await invoke('config_set_panel_url',{url:sites[site]});
  const ready=async()=>{
    for(let i=0;i<90;i++) {
      await sleep(500);
      try {
        const s=await panel.evaluate(`({url:location.origin+location.pathname,ready:document.readyState,api:!!window.__DSFLOAT__,pending:!!window.__dsfloatNavigationPending,input:!!window.__DSFLOAT__?.inspect().hasInput})`);
        if(s.ready==='complete' && s.api && !s.pending && s.input && new URL(s.url).hostname===new URL(sites[site]).hostname) return s;
      } catch { /* Navigation destroys the previous execution context. */ }
    }
    throw Error('Site is not ready or requires login. No question submitted.');
  };
  await ready();
  await invoke('result_new_session');
  report.page=await ready();
  await sleep(2000);
  async function ask(question, marker) {
    if(await invoke('plugin:window|is_visible',{label:'panel'})) {
      await invoke('toggle_panel');
      await sleep(500);
    }
    assert.equal(await invoke('plugin:window|is_visible',{label:'panel'}),false,'This acceptance requires the real panel to be hidden.');
    const started=Date.now();
    await invoke('result_ask',{text:question}); // Never resubmit automatically.
    let streamed=false;
    while(Date.now()-started<timeout) {
      await sleep(500);
      const s=await card.evaluate(`(()=>{const t=[...document.querySelectorAll('#turns .turn')].at(-1);return {question:t?.querySelector('.ask')?.textContent,answer:t?.querySelector('.md')?.innerText||'',notes:[...t?.querySelectorAll('.note')||[]].map(n=>n.textContent),streaming:!!t?.querySelector('.caret'),retry:!!t?.querySelector('[data-send-retry]'),html:t?.querySelector('.md')?.innerHTML}})()`);
      if(s.question!==question) continue;
      report.lastSample=s;
      streamed ||= s.streaming && !!s.answer;
      if(s.notes.includes('已完成') && !s.streaming) {
        const hidden=!(await invoke('plugin:window|is_visible',{label:'panel'}));
        assert(hidden,'Panel became visible; hidden rendering acceptance is inconclusive.');
        assert(s.answer.includes(marker), 'Completed card lacks the answer marker: '+JSON.stringify(s));
        return {...s,panelHiddenAtCompletion:hidden,streamObserved:streamed,elapsedMs:Date.now()-started};
      }
      if(s.retry || s.notes.some(n=>/超时|不确定|失败|已取消/.test(n))) throw Error(JSON.stringify(s));
    }
    throw Error('Acceptance wait timed out. No automatic retransmission.');
  }
  const marker=`SAME-${run}-${site}`;
  const question=`[真实验收 ${run}] 计算17+25，只输出42和标记 ${marker}。`;
  for(const label of ['first','repeat']) {
    const result=await ask(question,marker);
    report.steps.push({label,...result});
    console.log(`${site}: ${label} completed (${result.elapsedMs}ms)`);
  }
  const richMarker=`RICH-${run}-${site}`;
  const expectedCode='def answer():\n    return 17 + 25\nprint(answer())';
  const richQuestion=`[富文本验收 ${run}] 请按顺序输出：无序列表两项“条目A”“条目B”；Markdown 表格表头“名称”“数值”，数据行“行A”“17”和“行B”“25”；一个 python 围栏代码块，原样保留以下三行及缩进：\n${expectedCode}\n末尾输出 ${richMarker}。`;
  const rich=await ask(richQuestion,richMarker);
  report.steps.push({label:'rich',...rich});
  report.format=await card.evaluate(`(()=>{const m=[...document.querySelectorAll('#turns .turn .md')].at(-1);return {code:m.querySelector('pre code')?.textContent,rows:[...m.querySelectorAll('table tr')].map(r=>[...r.querySelectorAll('th,td')].map(c=>c.textContent.trim())),items:[...m.querySelectorAll('ul>li,ol>li')].map(n=>n.textContent.trim())}})()`);
  report.rawAnswer=await panel.evaluate(`({url:location.origin+location.pathname,snapshot:window.__DSFLOAT__.snapshotConversation(${JSON.stringify(richQuestion)},false),codeBlocks:[...document.querySelectorAll('[data-code-block],model-response pre')].map(n=>n.outerHTML)})`);
  assert.equal(report.format.code?.replace(/\r\n/g,'\n').trimEnd(),expectedCode,'Code lines or indentation differ.');
  assert.deepEqual(report.format.rows,[['名称','数值'],['行A','17'],['行B','25']],'Table cells differ.');
  assert.deepEqual(report.format.items,['条目A','条目B'],'List items differ.');
  await card.evaluate(`[...document.querySelectorAll('[data-favorite]')].at(-1).click()`);
  for(let i=0;i<20;i++) {
    report.favorite=(await invoke('favorites_list')).find(f=>f.question===richQuestion);
    if(report.favorite) break;
    await sleep(250);
  }
  assert(report.favorite,'Favorite UI did not persist the real answer.');
  const markdown=report.favorite.answer_markdown;
  assert(markdown.includes(richMarker) && markdown.includes(expectedCode) && /`{3,}/.test(markdown) && markdown.includes('|'), 'Favorite lost Markdown content.');
  await card.evaluate(`[...document.querySelectorAll('[data-export]')].at(-1).click()`);
  const exports=path.join(store,'exports');
  for(let i=0;i<20;i++) {
    report.export=fs.existsSync(exports)?fs.readdirSync(exports).find(name=>name.endsWith('.md') && fs.readFileSync(path.join(exports,name),'utf8').includes(richMarker)):undefined;
    if(report.export) break;
    await sleep(250);
  }
  assert(report.export,'Export UI did not produce the test answer in isolated storage.');
  const exported=fs.readFileSync(path.join(exports,report.export),'utf8');
  assert(exported.includes(markdown),'Export differs from the favorite Markdown.');
  report.outcome='pass';
} catch(error) {
  report.outcome='failed';
  report.error=String(error.stack||error);
  if(panel)try{report.failureDom=await panel.evaluate(`(()=>{const n=[...document.querySelectorAll('[data-dil-message-id],model-response')].at(-1);return {url:location.origin+location.pathname,html:n?.outerHTML,body:document.body.innerText.slice(-6000)}})()`);}catch{}
  process.exitCode=1;
} finally {
  if(invoke && original) {
    try {await invoke('config_set_panel_url',{url:original});report.restored=original;} catch(e) {report.restoreError=String(e);process.exitCode=1;}
  }
  clients.forEach(c=>c.close());
  report.finished=new Date().toISOString();
  fs.writeFileSync(output,JSON.stringify(report,null,2));
  console.log(JSON.stringify({site,outcome:report.outcome,error:report.error,report:output}));
}
