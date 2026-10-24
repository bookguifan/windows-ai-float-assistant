// 仅连接使用隔离LOCALAPPDATA配置（panel_url=https://workflow-fixture.test/）启动的测试实例。
// 所有模拟站点请求由CDP截断并返回本地HTML，绝不发送到真实AI服务。
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inflateSync} from 'node:zlib';
import {readFileSync} from 'node:fs';
import path from 'node:path';
const FIXTURE='https://workflow-fixture.test/';
const delay=ms=>new Promise(r=>setTimeout(r,ms));
let pages=[];
for(let i=0;i<40;i++){try{pages=await(await fetch('http://localhost:9444/json/list')).json()}catch{}if(pages.some(p=>p.url.endsWith('/poc/ball.html'))&&pages.some(p=>p.url.startsWith(FIXTURE)))break;await delay(200)}
async function connect(target){assert(target,'Required test page missing');const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true})});let id=0;const pending=new Map();const handlers=new Map();ws.addEventListener('message',event=>{const message=JSON.parse(event.data);if(message.id){pending.get(message.id)?.(message);pending.delete(message.id)}else handlers.get(message.method)?.(message.params)});const send=(method,params={})=>new Promise((resolve,reject)=>{const next=++id;const timer=setTimeout(()=>{pending.delete(next);reject(Error('CDP timeout: '+method))},6000);pending.set(next,message=>{clearTimeout(timer);message.error?reject(Error(JSON.stringify(message.error))):resolve(message.result)});ws.send(JSON.stringify({id:next,method,params}))});const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!r.exceptionDetails,JSON.stringify(r.exceptionDetails));return r.result.value};return{send,evaluate,on:(event,handler)=>handlers.set(event,handler),close:()=>ws.close()}}
const ball=await connect(pages.find(p=>p.url.endsWith('/poc/ball.html')));
const panel=await connect(pages.find(p=>p.url.startsWith(FIXTURE)));
const result=await connect(pages.find(p=>p.url.endsWith('/poc/result.html')));
const menu=await connect(pages.find(p=>p.url.endsWith('/poc/menu.html')));
const shot=await connect(pages.find(p=>p.url.endsWith('/poc/shot.html')));
let cursorToRestore;
let sequence=0;let interceptionError;
function fixtureHtml(id){return `<!doctype html><meta charset="utf-8"><title>Local workflow fixture</title><body style="margin:0;background:rgb(20,80,160);height:100vh"><textarea style="display:none"></textarea><script>
addEventListener('DOMContentLoaded',()=>{
 const state=window.__fixture={documentId:${id},mode:'success',question:'',input:'',fills:[],submits:[],snapshots:[],imageChunks:[],attachments:[]};
 Object.assign(window.__DSFLOAT__,{shotReset(){state.imageChunks=[]},shotPush(data,index){if(index!==state.imageChunks.length)return false;state.imageChunks.push(data);return true},shotCommit(){state.attachments.push(state.imageChunks.join(''));return true},hasAttachment(){return state.attachments.length>0},fill(text){state.fills.push(text);if(state.mode==='fill-failure')return false;state.question=state.input=text;return true},submit(text){if(state.mode==='submit-failure')return 'no-send-button';state.submits.push(text);state.input='';return 'ok'},sendEvidence(){return state.submits.length?'sent':'unknown'},stillHasText(){return !!state.input},pressEnter(){},snapshotConversation(prompt){state.snapshots.push(prompt);const pending=state.mode==='pending';const answer='本地模拟回答：仅用于验证结果回流与会话隔离，不发送任何真实内容。';return {ok:true,requestBound:state.submits.length>0,url:location.href,texts:pending?[state.question]:[state.question,answer],roles:pending?['user']:['user','assistant'],thinking:[],blocks:pending?1:2,total:state.question.length+(pending?0:answer.length),container:'fixture',streaming:pending,pending,done:!pending,reasoning:false}},newChat(){throw Error('native navigation should replace newChat click')}});

});</script></body>`}
async function until(predicate,message,timeout=12000){const start=Date.now();while(Date.now()-start<timeout){if(interceptionError)throw interceptionError;if(await predicate())return;await delay(100)}throw Error(message)}
try{
 const config=await ball.evaluate(`__TAURI_INTERNALS__.invoke('config_get')`);
 assert.equal(config.panel_url,FIXTURE,'Refusing to operate on a real user profile');
 panel.on('Fetch.requestPaused',async event=>{try{if(event.resourceType==='Document'){assert(event.request.url.startsWith(FIXTURE));await panel.send('Fetch.fulfillRequest',{requestId:event.requestId,responseCode:200,responseHeaders:[{name:'Content-Type',value:'text/html; charset=utf-8'}],body:Buffer.from(fixtureHtml(++sequence)).toString('base64')})}else await panel.send('Fetch.failRequest',{requestId:event.requestId,errorReason:'BlockedByClient'})}catch(error){interceptionError=error}});
 await panel.send('Fetch.enable',{patterns:[{urlPattern:'*'}]});
 await panel.send('Page.navigate',{url:FIXTURE});
 await until(()=>panel.evaluate(`!!window.__fixture&&!window.__dsfloatNavigationPending`),'fixture ready');
 const initialDocument=await panel.evaluate(`__fixture.documentId`);
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_new_session')`);
 await until(()=>panel.evaluate(`!!window.__fixture&&__fixture.documentId!==${initialDocument}&&!window.__dsfloatNavigationPending`),'fresh fixture session');
 await panel.evaluate(`__fixture.mode='submit-failure'`);
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_ask',{text:'本地失败重试夹具'})`);
 await until(()=>result.evaluate(`!!document.querySelector('[data-send-retry]')`),'failed send should expose retry');
 assert.equal(await panel.evaluate(`__fixture.submits.length`),0);
 console.log('PASS failed send is visible without waiting for answer timeout');
 await panel.evaluate(`__fixture.mode='success'`);
 await result.evaluate(`(()=>{const b=document.querySelector('[data-send-retry]');b.click();b.click()})()`);
 await until(()=>result.evaluate(`document.getElementById('turns').textContent.includes('本地模拟回答')&&!document.querySelector('.caret')`),'retry answer should complete');
 assert.equal(await panel.evaluate(`__fixture.submits.length`),1);
 assert.equal(await panel.evaluate(`__fixture.submits[0]`),'本地失败重试夹具');
 console.log('PASS retry sends the original prompt once and renders the local answer');
 if(process.argv.includes('--favorites')) {
  await result.evaluate(`document.querySelector('[data-favorite]').click()`);
  await until(()=>result.evaluate(`document.getElementById('saveStatus').textContent.includes('已收藏')`),'favorite saved');
  await result.evaluate(`document.querySelector('[data-favorite]').click()`);
  await delay(200);
  const favorites=await result.evaluate(`__TAURI_INTERNALS__.invoke('favorites_list')`);
  const saved=favorites.find(f=>f.question==='本地失败重试夹具');assert(saved);
  assert.equal(favorites.filter(f=>f.question===saved.question&&f.answer_markdown===saved.answer_markdown).length,1);
  await result.evaluate(`document.querySelector('[data-export]').click()`);
  await until(()=>result.evaluate(`document.getElementById('saveStatus').textContent.includes('.md')`),'single answer exported');
  const exportPath=await result.evaluate(`document.getElementById('saveStatus').textContent.match(/[A-Z]:[\\\\/].*?\.md/i)?.[0]`);
  assert(exportPath,'Export path must be visible');
  const allowedRoot=path.resolve(fileURLToPath(new URL('../.test-artifacts',import.meta.url)))+path.sep;
  assert(path.resolve(exportPath).startsWith(allowedRoot),'Refusing to read a non-test export');
  const markdown=readFileSync(exportPath,'utf8');assert(markdown.includes(saved.question));assert(markdown.includes(saved.answer_markdown));assert(!markdown.includes('重试发送'));
  await result.evaluate(`document.getElementById('saveStatus').textContent='';document.getElementById('btnExport').click()`);
  await until(()=>result.evaluate(`document.getElementById('saveStatus').textContent.includes('.md')`),'whole conversation exported');
  const wholePath=await result.evaluate(`document.getElementById('saveStatus').textContent.match(/[A-Z]:[\\\\/].*?\.md/i)?.[0]`);
  assert(wholePath&&wholePath!==exportPath,'Repeated exports must not overwrite the previous file');
  assert(path.resolve(wholePath).startsWith(allowedRoot));assert(readFileSync(wholePath,'utf8').includes(saved.answer_markdown));
  const favoritePath=await result.evaluate(`__TAURI_INTERNALS__.invoke('favorites_export',{id:${saved.id}})`);
  assert(path.resolve(favoritePath).startsWith(allowedRoot));assert(readFileSync(favoritePath,'utf8').includes(saved.question));
  await result.evaluate(`document.getElementById('btnFavorites').click()`);
  await until(()=>result.evaluate(`document.getElementById('favoritesView').textContent.includes('本地模拟回答')`),'saved answer browsable');
  assert.equal(await result.evaluate(`getComputedStyle(document.getElementById('turns')).display`),'none');
  await result.evaluate(`document.getElementById('btnFavorites').click()`);
  await result.evaluate(`__TAURI_INTERNALS__.invoke('favorites_delete',{id:${saved.id}})`);
  assert(!(await result.evaluate(`__TAURI_INTERNALS__.invoke('favorites_list')`)).some(f=>f.id===saved.id));
  console.log('PASS actual favorite save/dedup/browse/delete and Markdown export use the isolated profile');
 }
 await panel.evaluate(`__fixture.mode='fill-failure'`);
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_ask',{text:'旧注入应在新会话取消'})`);
 await until(()=>panel.evaluate(`__fixture.fills.includes('旧注入应在新会话取消')`),'old fill started');
 const oldDocument=await panel.evaluate(`__fixture.documentId`);
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_new_session')`);
 await until(()=>panel.evaluate(`!!window.__fixture&&__fixture.documentId!==${oldDocument}`),'new session must navigate to a new document');
 await delay(1200);
 assert.equal(await panel.evaluate(`__fixture.fills.length`),0);
 assert.equal(await panel.evaluate(`__fixture.submits.length`),0);
 assert.equal(await panel.evaluate(`__fixture.snapshots.length`),0);
 assert.equal(await result.evaluate(`document.querySelectorAll('#turns .turn').length`),0);
 console.log('PASS new session cancels delayed fill, readback and stale result rendering');
 if(process.argv.includes('--sites')) {
  await menu.evaluate(`(()=>{const set=window.__dsfMenuSet;window.__dsfMenuSet=view=>{window.__fixtureMenu=view;return set(view)}})()`);
  await panel.evaluate(`__fixture.mode='fill-failure'`);
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_ask',{text:'换站点应取消旧注入'})`);
  await until(()=>panel.evaluate(`__fixture.fills.length>0`),'old site fill started');
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32})`);
  await until(()=>menu.evaluate(`!!window.__fixtureMenu`),'menu view received');
  const generation=await menu.evaluate(`__fixtureMenu.generation`);
  await menu.evaluate(`__TAURI_INTERNALS__.invoke('context_menu_switch_site',{generation:${generation},url:'${FIXTURE}alternate'})`);
  await until(()=>panel.evaluate(`location.href==='${FIXTURE}alternate'&&!!window.__fixture&&!window.__dsfloatNavigationPending`),'site navigation completed');
  await delay(1200);
  assert.equal(await panel.evaluate(`__fixture.fills.length+__fixture.submits.length+__fixture.snapshots.length`),0);
  assert.equal((await ball.evaluate(`__TAURI_INTERNALS__.invoke('config_get')`)).panel_url,FIXTURE+'alternate');
  assert(await menu.evaluate(`__TAURI_INTERNALS__.invoke('context_menu_switch_site',{generation:${generation},url:'${FIXTURE}'}).then(()=>false,()=>true)`),'Consumed menu generation must reject a repeated switch');
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('config_set_panel_url',{url:'${FIXTURE}'})`);
  await until(()=>panel.evaluate(`location.href==='${FIXTURE}'&&!!window.__fixture&&!window.__dsfloatNavigationPending`),'restore fixture site');
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_new_session')`);
  await until(()=>result.evaluate(`document.querySelectorAll('#turns .turn').length===0`),'clear cancelled site turn');
  console.log('PASS menu site switch cancels old work, persists selected URL and rejects repeated action');
 }
 if(process.argv.includes('--timeout')) {
  await panel.evaluate(`__fixture.mode='pending'`);
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_ask',{text:'本地超时与继续读取夹具'})`);
  await until(()=>result.evaluate(`document.getElementById('turns').textContent.includes('读取中')`),'confirmed send must enter reading state');
  console.log('PASS confirmed send enters reading; waiting for the real answer timeout');
  await until(()=>result.evaluate(`Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='继续读取')`),'answer timeout must expose read-only retry',75000);
  assert.equal(await panel.evaluate(`__fixture.submits.length`),1);
  assert.equal(await result.evaluate(`!!document.querySelector('[data-send-retry]')`),false);
  await panel.evaluate(`__fixture.mode='success'`);
  await result.evaluate(`(()=>{const b=Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='继续读取');b.click();b.click()})()`);
  await until(()=>result.evaluate(`document.getElementById('turns').textContent.includes('本地模拟回答')&&!document.querySelector('.caret')`),'continue reading must complete the answer');
  assert.equal(await panel.evaluate(`__fixture.submits.length`),1);
  console.log('PASS actual answer timeout resumes reading once without resending');
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_new_session')`);
  await until(()=>panel.evaluate(`!!window.__fixture&&__fixture.submits.length===0`),'reset timeout fixture');
 }
 if(process.argv.includes('--screenshots')) {
  const probe=(...args)=>{const out=spawnSync('pwsh',['-NoProfile','-File',fileURLToPath(new URL('./Get-FixtureWindow.ps1',import.meta.url)),...args],{encoding:'utf8'});assert.equal(out.status,0,out.stderr);return out.stdout.trim()?JSON.parse(out.stdout):null};
  if(!await ball.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'panel'})`))await ball.evaluate(`__TAURI_INTERNALS__.invoke('toggle_panel')`);
  await shot.evaluate(`(()=>{const reset=window.__dsfShotReset;window.__dsfShotReset=(id,revision,mode)=>{window.__fixtureCapture={id,revision,mode};return reset(id,revision,mode)};const select=window.__dsfShotReselect;window.__dsfShotReselect=(id,revision)=>{window.__fixtureCapture={...window.__fixtureCapture,id,revision};return select(id,revision)}})()`);
  const panelBox=probe('-Window','panel','-MoveCursor');cursorToRestore={x:panelBox.cursorX,y:panelBox.cursorY};
  const begin=async(action='screenshot')=>{await until(async()=>{await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32})`);await delay(100);return menu.evaluate(`!document.getElementById('screenshot').disabled`)},'previous capture cleaned up');await menu.evaluate(`document.getElementById('${action}').click()`);await until(()=>shot.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'shot'})`),'shot overlay visible')};
  const select=async()=>{const overlay=probe('-Window','shot');const size=await shot.evaluate(`({scale:devicePixelRatio})`);const x=(panelBox.x+30-overlay.x)/size.scale,y=(panelBox.y+60-overlay.y)/size.scale;await shot.send('Input.dispatchMouseEvent',{type:'mousePressed',x,y,button:'left',clickCount:1});await shot.send('Input.dispatchMouseEvent',{type:'mouseReleased',x:x+120/size.scale,y:y+80/size.scale,button:'left',clickCount:1});await until(()=>shot.evaluate(`!document.getElementById('preview').hidden&&!document.getElementById('confirm').disabled`),'PNG preview ready')};
  await begin();await select();
  assert.equal(await panel.evaluate(`__fixture.attachments.length`),0);
  const png=Buffer.from(await shot.evaluate(`document.getElementById('preview-image').src.split(',')[1]`),'base64');assert.equal(png.readUInt32BE(16),120);assert.equal(png.readUInt32BE(20),80);
  const chunks=[];for(let pos=8;pos<png.length;){const length=png.readUInt32BE(pos);if(png.toString('ascii',pos+4,pos+8)==='IDAT')chunks.push(png.subarray(pos+8,pos+8+length));pos+=length+12}const first=inflateSync(Buffer.concat(chunks));assert(Math.abs(first[1]-20)<12&&Math.abs(first[2]-80)<12&&Math.abs(first[3]-160)<12,'Preview must contain the controlled blue fixture, not desktop/private content');
  console.log('PASS actual screenshot PNG is previewed without uploading');
  await shot.evaluate(`document.getElementById('reselect').click()`);await until(()=>shot.evaluate(`document.getElementById('preview').hidden`),'reselect returns to selection');await select();
  assert.equal(await panel.evaluate(`__fixture.attachments.length`),0);
  await shot.evaluate(`(()=>{const b=document.getElementById('confirm');b.click();b.click()})()`);
  await until(()=>panel.evaluate(`__fixture.attachments.length===1`),'confirmed image uploads once');
  assert.equal(await panel.evaluate(`__fixture.submits.length`),0);

  console.log('PASS reselect preserves draft isolation and confirm attaches exactly once without sending');
  await until(()=>shot.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'shot'}).then(v=>!v)`),'shot confirmation completes');await delay(300);
  await begin();await select();const stale=await shot.evaluate(`__fixtureCapture`);
  const previousDocument=await panel.evaluate(`__fixture.documentId`);await ball.evaluate(`__TAURI_INTERNALS__.invoke('result_new_session')`);await until(()=>panel.evaluate(`!!window.__fixture&&__fixture.documentId!==${previousDocument}`),'new fixture document');
  const staleRejected=await ball.evaluate(`__TAURI_INTERNALS__.invoke('shot_confirm',{captureId:${stale.id},revision:${stale.revision}}).then(()=>false,()=>true)`);
  assert(staleRejected,'Old preview confirmation must be rejected after new session');
  assert.equal(await panel.evaluate(`__fixture.attachments.length`),0);
  console.log('PASS new session invalidates old screenshot preview and prevents delayed attachment');
  if(process.argv.includes('--ocr')) {
   await begin('ocr');await select();
   assert.equal(await panel.evaluate(`__fixture.attachments.length+__fixture.submits.length`),0);
   await shot.evaluate(`(()=>{const b=document.getElementById('confirm');b.click();b.click()})()`);
   await until(()=>result.evaluate(`document.getElementById('turns').textContent.includes('本地模拟回答')&&!document.querySelector('.caret')`),'confirmed OCR answer should reach result card');
   assert.equal(await panel.evaluate(`__fixture.attachments.length`),1);
   assert.equal(await panel.evaluate(`__fixture.submits.length`),1);
   console.log('PASS OCR preview waits for confirmation, attaches once and starts one readback workflow');
  }
 }
 console.log('PASS all fixture requests stayed local and profile configuration was unchanged');
}finally{if(cursorToRestore)spawnSync('pwsh',['-NoProfile','-File',fileURLToPath(new URL('./Get-FixtureWindow.ps1',import.meta.url)),'-Window','restore','-X',String(cursorToRestore.x),'-Y',String(cursorToRestore.y)]);ball.close();panel.close();result.close();menu.close();shot.close()}
