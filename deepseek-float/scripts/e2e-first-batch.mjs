// 仅连接工作区 .test-artifacts/first-batch-native 配置的隔离实例。
// 运行前创建 isolation.marker，使用 custom-protocol debug 程序及本地9444调试端口。
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync,rmdirSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const profile = fileURLToPath(new URL('../.test-artifacts/first-batch-native/',import.meta.url));
const root = path.join(profile,'com.deepseekfloat.desktop');
assert.equal(readFileSync(path.join(profile,'isolation.marker'),'utf8').trim(),'first-batch-isolated');
const fixture = 'https://workflow-fixture.test/';
const delay = ms => new Promise(resolve=>setTimeout(resolve,ms));
let targets = [];
for(let i=0;i<50;i++) {
  try {targets = await (await fetch('http://localhost:9444/json/list')).json();} catch {}
  if(['/poc/ball.html','/poc/settings.html','/poc/result.html','/poc/menu.html'].every(s=>targets.some(t=>t.url.endsWith(s)))) break;
  await delay(200);
}
async function connect(suffix) {
  const target = targets.find(t=>t.url.endsWith(suffix)); assert(target,`Missing page ${suffix}`);
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true});});
  let id=0; const pending=new Map();
  ws.addEventListener('message',event=>{const msg=JSON.parse(event.data);if(msg.id){pending.get(msg.id)?.(msg);pending.delete(msg.id);}});
  async function evaluate(expression) {
    const next=++id;
    const reply=await new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{pending.delete(next);reject(Error('CDP timeout'));},8000);
      pending.set(next,msg=>{clearTimeout(timer);msg.error?reject(Error(JSON.stringify(msg.error))):resolve(msg.result);});
      ws.send(JSON.stringify({id:next,method:'Runtime.evaluate',params:{expression,awaitPromise:true,returnByValue:true}}));
    });
    assert(!reply.exceptionDetails,JSON.stringify(reply.exceptionDetails));return reply.result.value;
  }
  return {evaluate,close:()=>ws.close()};
}
async function until(predicate,message) {
  for(let i=0;i<100;i++){if(await predicate())return;await delay(100);} throw Error(message);
}
const ball=await connect('/poc/ball.html');
const settings=await connect('/poc/settings.html');
const result=await connect('/poc/result.html');
const menu=await connect('/poc/menu.html');
const invoke=(client,command,args={})=>client.evaluate(`__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`);
const blocked = path.join(root,'prompts.json.tmp');
let blockedCreated = false;
try {
  const config=await invoke(ball,'config_get');
  assert.equal(config.panel_url,fixture,'Refusing real user profile');
  assert(config.custom_sites.some(s=>s.label==='夹具标记'),'Refusing unmarked profile');
  assert.equal(JSON.parse(readFileSync(path.join(root,'config.json'),'utf8')).panel_url,fixture);
  await invoke(ball,'ball_contextmenu',{x:32,y:32});
  await until(()=>invoke(menu,'plugin:window|is_visible',{label:'context-menu'}),'menu ready');
  await menu.evaluate(`document.getElementById('settings').click()`);
  await until(()=>settings.evaluate(`document.querySelectorAll('#list .row').length>=4&&!document.getElementById('site-save').disabled`),'settings baseline ready');
  if (process.argv.includes('--verify-restart')) {
    assert(await settings.evaluate(`document.getElementById('list').textContent.includes('本地重试')`));
    assert(await settings.evaluate(`document.getElementById('custom-sites-list').textContent.includes('夹具标记')`));
    assert.equal((await invoke(result,'favorites_list')).length,2);
    console.log('PASS restarted native process restores saved prompts, custom sites and legacy favorites');
  } else {
  await settings.evaluate(`document.querySelector('[data-act="hide"]').click()`);
  await until(()=>settings.evaluate(`document.getElementById('prompt-save-status').textContent==='已保存'`),'initial prompt save');
  const before=readFileSync(path.join(root,'prompts.json'),'utf8');
  const oldPrompts=JSON.parse(before);
  mkdirSync(blocked);blockedCreated=true;
  await settings.evaluate(`document.querySelector('[data-act="edit"]').click();document.getElementById('f-label').value='本地重试';document.getElementById('f-save').click();`);
  await until(()=>settings.evaluate(`document.getElementById('prompt-save-status').textContent.includes('保存失败')`),'disk failure visible');
  assert.equal(readFileSync(path.join(root,'prompts.json'),'utf8'),before);
  await settings.evaluate(`__dsfSettingsInit({prompts:${JSON.stringify(oldPrompts)}})`);
  assert(await settings.evaluate(`document.getElementById('list').textContent.includes('本地重试')`));
  rmdirSync(blocked);blockedCreated=false;
  await settings.evaluate(`document.getElementById('prompt-save-retry').click()`);
  await until(()=>settings.evaluate(`document.getElementById('prompt-save-status').textContent==='已保存'`),'retry saved');
  assert(JSON.parse(readFileSync(path.join(root,'prompts.json'),'utf8')).some(p=>p.label==='本地重试'));
  console.log('PASS native disk failure preserves previous prompts and retry writes the retained edit');

  await settings.evaluate(`document.getElementById('site-label').value='本地站点甲';document.getElementById('site-url').value='https://workflow-fixture.test/custom-a';document.getElementById('site-save').click();`);
  await until(()=>settings.evaluate(`document.getElementById('sites-save-status').textContent==='已保存'`),'site saved');
  let disk=JSON.parse(readFileSync(path.join(root,'config.json'),'utf8'));
  assert(disk.custom_sites.some(s=>s.label==='本地站点甲'));assert.equal(disk.panel_url,fixture);
  await invoke(ball,'ball_contextmenu',{x:32,y:32});
  await until(()=>menu.evaluate(`Array.from(document.querySelectorAll('[data-site]')).some(b=>b.textContent.includes('本地站点甲'))`),'custom site appears in menu');
  await menu.evaluate(`document.getElementById('switchSite').click()`);
  const menuLayout=await menu.evaluate(`({width:innerWidth,height:innerHeight,scrollHeight:document.getElementById('siteList').scrollHeight,clientHeight:document.getElementById('siteList').clientHeight})`);
  assert.equal(menuLayout.width,200);assert.equal(menuLayout.height,180);assert(menuLayout.scrollHeight>menuLayout.clientHeight);
  await menu.evaluate(`document.getElementById('siteBack').click()`);
  await menu.evaluate(`window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))`);
  await until(async()=>!await invoke(menu,'plugin:window|is_visible',{label:'context-menu'}),'menu closed');
  await invoke(ball,'ball_contextmenu',{x:32,y:32});
  await until(()=>invoke(menu,'plugin:window|is_visible',{label:'context-menu'}),'menu reopened');
  await menu.evaluate(`document.getElementById('settings').click()`);
  await until(()=>settings.evaluate(`document.querySelectorAll('[data-site-action="delete"]').length===2`),'saved sites reloaded');
  await settings.evaluate(`window.confirm=()=>true;document.querySelectorAll('[data-site-action="delete"]')[1].click()`);
  await until(()=>settings.evaluate(`document.getElementById('sites-save-status').textContent==='已保存'&&document.querySelectorAll('[data-site-action="delete"]').length===1`),'site deletion saved');
  disk=JSON.parse(readFileSync(path.join(root,'config.json'),'utf8'));assert.equal(disk.custom_sites.length,1);assert.equal(disk.panel_url,fixture);
  console.log('PASS native custom site persists, appears in compact menu, and removal does not navigate');

  await result.evaluate(`document.getElementById('btnFavorites').click()`);
  await until(()=>result.evaluate(`!!document.getElementById('favorites-search')`),'favorites loaded');
  assert(await result.evaluate(`document.getElementById('favoritesView').textContent.includes('收藏夹具')`));
  await result.evaluate(`const search=document.getElementById('favorites-search');search.value='  rUsT  ';search.dispatchEvent(new Event('input'));`);
  assert.equal(await result.evaluate(`document.querySelectorAll('[data-favorite-copy]').length`),1);
  await result.evaluate(`document.getElementById('favorites-search-clear').click()`);
  assert.equal(await result.evaluate(`document.querySelectorAll('[data-favorite-copy]').length`),2);
  assert.equal((await invoke(result,'favorites_list')).length,2);
  console.log('PASS native favorites load real fixture data and search without changing stored content');
  console.log('PASS all first-batch checks used only the isolated workspace profile');
  }
} finally {
  if(blockedCreated)rmdirSync(blocked);
  ball.close();settings.close();result.close();menu.close();
}
