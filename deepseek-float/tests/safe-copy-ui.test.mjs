import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
const tick=()=>new Promise(r=>setTimeout(r,0));
test('compatibility mode requires native consent and safe/off revoke without legacy config writes',async()=>{
 const p=page((command,args)=>command==='safe_copy_authorize'
  ?Promise.resolve({available:true,enabled:true,mode:args.mode})
  :command==='safe_copy_revoke'?Promise.resolve({available:true,enabled:false,mode:args.mode}):undefined);
 try{await tick();const mode=p.d.querySelector('#safe-copy-mode');assert.ok(mode);
  assert.equal(mode.value,'safe');mode.value='compatibility';mode.dispatchEvent(new p.w.Event('change'));await tick();
  assert.equal(mode.value,'compatibility');assert.deepEqual(JSON.parse(JSON.stringify(p.calls.find(c=>c.command==='safe_copy_authorize').args)),{mode:'compatibility'});
  mode.value='off';mode.dispatchEvent(new p.w.Event('change'));await tick();assert.equal(mode.value,'off');
  assert.equal(p.calls.filter(c=>c.command==='config_set_clipboard_fallback').length,0);
 }finally{p.close();}
});
function page(handler=()=>undefined){
 const dom=new JSDOM(readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8'),{runScripts:'outside-only',url:'https://tauri.localhost'});
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{
  calls.push({command,args});return handler(command,args)??Promise.resolve(command==='config_get'?{clipboard_fallback_enabled:true}:command==='safe_copy_status'?{available:true,enabled:false}:true);
 }};dom.window.alert=()=>{};dom.window.confirm=()=>{throw Error('Frontend confirmation must not authorize');};
 for(const s of dom.window.document.querySelectorAll('script'))dom.window.eval(s.textContent);
 dom.window.__dsfSettingsInit({prompts:[]});
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
const choose=(p,value)=>{const el=p.d.querySelector('#safe-copy-mode');el.value=value;el.dispatchEvent(new p.w.Event('change'));return el;};
test('legacy config and imported flags cannot grant compatibility consent',async()=>{
 const p=page();try{await tick();assert.equal(p.d.querySelector('#safe-copy-mode').value,'safe');p.w.__dsfSettingsDataRestored({prompts:[],clipboard_fallback_enabled:true,copy_mode:'compatibility'});await tick();assert.equal(p.d.querySelector('#safe-copy-mode').value,'safe');assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
});
test('compatibility warning describes shared clipboard risk and supported snapshot limits',async()=>{
 const p=page();try{await tick();const hint=p.d.querySelector('#safe-copy-hint').textContent;for(const pattern of [/延迟渲染.*同步等待/,/不能中断仍未返回/,/不保证.*无损/,/HTML.*RTF.*图片/,/跳过恢复/,/一次 Ctrl\+Insert/,/不代表已经通过兼容验收/])assert.match(hint,pattern);}finally{p.close();}
});
test('native denial leaves compatibility off without invoking the legacy setting',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve({available:true,enabled:false,mode:'safe'}):undefined);
 try{await tick();choose(p,'compatibility');await tick();assert.equal(p.d.querySelector('#safe-copy-mode').value,'safe');assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);assert.equal(p.calls.filter(c=>c.command==='config_set_clipboard_fallback').length,0);}finally{p.close();}
});
test('pending native consent prevents duplicate requests',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();const el=choose(p,'compatibility');assert.equal(el.disabled,true);choose(p,'compatibility');assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);done({available:true,enabled:false,mode:'safe'});await tick();assert.equal(el.disabled,false);assert.equal(el.value,'safe');}finally{p.close();}
});
test('native status push cannot cancel or unlock a pending mode change',async()=>{
 for(const requested of ['compatibility','off','safe']){
  let done;const p=page(c=>['safe_copy_authorize','safe_copy_revoke'].includes(c)?new Promise(r=>done=r):undefined);
  try{await tick();const el=choose(p,requested);
   p.w.__dsfSafeCopyStatus({available:true,enabled:false,mode:'safe',last_error:'old transaction'});
   assert.equal(el.disabled,true,'status push must not unlock an outstanding native request');
   choose(p,requested);assert.equal(p.calls.filter(c=>['safe_copy_authorize','safe_copy_revoke'].includes(c.command)).length,1);
   done({available:true,enabled:requested==='compatibility',mode:requested});await tick();
   assert.equal(el.value,requested);assert.equal(el.disabled,false);
  }finally{p.close();}
 }
});
test('authorization errors fail closed with a visible reason and allow retry',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.reject(Error('native denied')):undefined);
 try{await tick();const el=choose(p,'compatibility');await tick();assert.equal(el.value,'safe');assert.equal(el.disabled,false);assert.match(p.d.querySelector('#safe-copy-status').textContent,/native denied/);}finally{p.close();}
});
test('late status read cannot override a newer native authorization',async()=>{
 let status,reads=0;const p=page(c=>c==='safe_copy_status'?(++reads===1?Promise.resolve({available:true,enabled:false,mode:'safe'}):new Promise(r=>status=r)):c==='safe_copy_authorize'?Promise.resolve({available:true,enabled:true,mode:'compatibility'}):undefined);
 try{await tick();p.w.__dsfSettingsDataRestored({prompts:[]});await tick();const el=choose(p,'compatibility');await tick();assert.equal(el.value,'compatibility');status({available:true,enabled:false,mode:'safe'});await tick();assert.equal(el.value,'compatibility');}finally{p.close();}
});
test('restore reflects runtime revocation rather than imported flags',async()=>{
 let enabled=true;const p=page(c=>c==='safe_copy_status'?Promise.resolve({available:true,enabled,mode:'compatibility'}):undefined);
 try{await tick();const el=p.d.querySelector('#safe-copy-mode');assert.equal(el.value,'compatibility');enabled=false;p.w.__dsfSettingsDataRestored({prompts:[]});await tick();await tick();assert.equal(el.value,'safe');assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
});
test('safe and off revoke without an authorization dialog',async()=>{
 const p=page((c,args)=>c==='safe_copy_revoke'?Promise.resolve({available:true,enabled:false,mode:args.mode}):undefined);
 try{await tick();for(const mode of ['off','safe']){choose(p,mode);await tick();assert.equal(p.d.querySelector('#safe-copy-mode').value,mode);}assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
});
test('transaction failure is visible without clipboard text',async()=>{
 const p=page();try{await tick();p.w.__dsfSafeCopyStatus({available:true,enabled:true,mode:'compatibility',last_error:'原剪贴板格式不支持'});assert.match(p.d.querySelector('#safe-copy-status').textContent,/原剪贴板格式不支持/);}finally{p.close();}
});

test('settings selection expiry removes preview and disables stale run controls',async()=>{
 const p=page();try{
  p.w.__dsfSettingsInit({text:'synthetic selection',selection_ticket:'1',selection_revision:'1',prompts:[{id:'test',label:'测试',instruction:'{text}'}]});
  assert.equal(p.d.querySelector('#text-preview').textContent,'synthetic selection');
  assert.equal(p.d.querySelector('[data-act="run"]').disabled,false);
  p.w.__dsfSettingsSelectionExpired();
  assert.equal(p.d.querySelector('#text-preview').classList.contains('preview-empty'),true);
  assert.equal(p.d.querySelector('[data-act="run"]').disabled,true);
 }finally{p.close();}
});
test('opening settings without an explicit selection cannot run an old preview',async()=>{
 const p=page();try{
  p.w.__dsfSettingsInit({text:'old synthetic',selection_ticket:'1',selection_revision:'1',prompts:[{id:'test',label:'测试',instruction:'{text}'}]});
  p.w.__dsfSettingsInit({prompts:[{id:'test',label:'测试',instruction:'{text}'}]});
  assert.equal(p.d.querySelector('[data-act="run"]').disabled,true);
 }finally{p.close();}
});

test('late preview from an older selection cannot replace the current selection',async()=>{
 const p=page();try{
  const prompts=[{id:'test',label:'测试',instruction:'{text}'}];
  p.w.__dsfSettingsInit({text:'new synthetic',selection_ticket:'3',selection_revision:'3',prompts});
  p.w.__dsfSettingsInit({text:'old synthetic',selection_ticket:'2',selection_revision:'2',prompts});
  assert.equal(p.d.querySelector('#text-preview').textContent,'new synthetic');
  p.w.__dsfSettingsSelectionExpired('2');
  assert.equal(p.d.querySelector('[data-act="run"]').disabled,false);
 }finally{p.close();}
});
test('settings run carries the exact snapshot ticket rather than implicit latest text',async()=>{
 const p=page();try{
  p.w.__dsfSettingsInit({text:'synthetic',selection_ticket:'7',selection_revision:'7',prompts:[{id:'test',label:'测试',instruction:'{text}'}]});
  p.d.querySelector('[data-act="run"]').click();await tick();
  const call=p.calls.find(x=>x.command==='settings_run');
  assert.equal(call.args.selectionTicket,'7');
 }finally{p.close();}
});
test('restore clears the delegated preview even without a separate native expiry event',async()=>{
 const p=page();try{
  const prompts=[{id:'test',label:'测试',instruction:'{text}'}];
  p.w.__dsfSettingsInit({text:'synthetic',selection_ticket:'1',selection_revision:'1',prompts});
  p.w.__dsfSettingsDataRestored({prompts});
  assert.equal(p.d.querySelector('[data-act="run"]').disabled,true);
  assert.equal(p.d.querySelector('#text-preview').classList.contains('preview-empty'),true);
 }finally{p.close();}
});

test('unavailable build stays disabled after restore even with imported flags',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve({available:false,enabled:true,mode:'compatibility'}):undefined);
 try{await tick();const el=p.d.querySelector('#safe-copy-mode');assert.equal(el.disabled,true);assert.equal(el.value,'safe');p.w.__dsfSettingsDataRestored({prompts:[],clipboard_fallback_enabled:true});await tick();await tick();assert.equal(el.disabled,true);choose(p,'compatibility');await tick();assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);assert.match(p.d.querySelector('#safe-copy-hint').textContent,/此构建未提供/);}finally{p.close();}
});

test('cold settings page asks for current native snapshot after handlers are ready',async()=>{
 const raw='  中文 😀\r\n  code <&>\r\n';
 const p=page(c=>c==='settings_ready'?Promise.resolve({text:raw,selection_ticket:'11',selection_revision:'11',prompts:[{id:'test',label:'test',instruction:'{text}'}]}):undefined);
 try{await tick();await tick();assert.equal(p.d.querySelector('#text-preview').textContent,raw);assert.equal(p.d.querySelector('[data-act="run"]').disabled,false);}finally{p.close();}
});
test('settings preview preserves raw leading trailing whitespace and CRLF',()=>{
 const p=page();try{const raw='  中文 😀\r\n  code <&>\r\n';p.w.__dsfSettingsInit({text:raw,selection_ticket:'12',selection_revision:'12',prompts:[]});assert.equal(p.d.querySelector('#text-preview').textContent,raw);}finally{p.close();}
});
test('same revision late init cannot resurrect an explicitly revoked preview',()=>{
 const p=page();try{const payload={text:'synthetic',selection_ticket:'9',selection_revision:'9',prompts:[{id:'test',label:'test',instruction:'{text}'}]};p.w.__dsfSettingsInit(payload);p.w.__dsfSettingsSelectionExpired('9');p.w.__dsfSettingsInit(payload);assert.equal(p.d.querySelector('[data-act="run"]').disabled,true);}finally{p.close();}
});
test('late cold-page snapshot cannot override newer live preview',async()=>{
 let ready;const p=page(c=>c==='settings_ready'?new Promise(r=>ready=r):undefined);
 try{await tick();p.w.__dsfSettingsInit({text:'new',selection_ticket:'13',selection_revision:'13',prompts:[]});assert.equal(typeof ready,'function');ready({text:'old',selection_ticket:'12',selection_revision:'12',prompts:[]});await tick();assert.equal(p.d.querySelector('#text-preview').textContent,'new');}finally{p.close();}
});

function toolbarPage(){
 const dom=new JSDOM(readFileSync(new URL('../poc/popup.html',import.meta.url),'utf8'),{runScripts:'outside-only',url:'http://tauri.localhost/poc/popup.html'});
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{calls.push({command,args});return Promise.resolve(true);}};
 for(const script of dom.window.document.querySelectorAll('script'))dom.window.eval(script.textContent);
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
test('toolbar action carries the displayed session ticket and repeated clicks fire once',()=>{
 const p=toolbarPage();try{const copy=p.d.querySelector('[data-preset="copy"]');copy.click();assert.equal(p.calls.length,0);p.w.__dsfResetFired('42');copy.click();copy.click();assert.equal(p.calls.length,1);assert.deepEqual(JSON.parse(JSON.stringify(p.calls[0].args)),{preset:'copy',selectionTicket:'42'});p.w.__dsfResetFired('43');copy.click();assert.equal(p.calls[1].args.selectionTicket,'43');}finally{p.close();}
});
test('first toolbar action uses the ticket installed before native show is acknowledged',async()=>{
 const p=toolbarPage();try{
  p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{p.calls.push({command,args});return Promise.resolve(true);};
  await p.w.__dsfPrepareSelection('41',[{id:'explain',label:'解释'}]);
  assert.equal(p.calls[0].command,'popup_ready');
  p.d.querySelector('[data-preset="explain"]').click();
  assert.deepEqual(p.calls.filter(c=>c.command==='popup_action').map(c=>JSON.parse(JSON.stringify(c.args))),
   [{preset:'explain',selectionTicket:'41'}]);
 }finally{p.close();}
});
test('late ready snapshot after a higher revision focus revocation cannot revive the preview',async()=>{
 let ready;const p=page(c=>c==='settings_ready'?new Promise(resolve=>ready=resolve):undefined);
 try{await tick();p.w.__dsfSettingsSelectionExpired('22');ready({text:'old synthetic',selection_ticket:'21',selection_revision:'21',prompts:[]});await tick();assert.equal(p.d.querySelector('#text-preview').classList.contains('preview-empty'),true);}finally{p.close();}
});

// A late unlock callback for A must never re-enable a consumed action for B.
test('old toolbar unlock timer cannot re-enable a replacement ticket',()=>{
 const p=toolbarPage();try{
  const timers=[];p.w.setTimeout=(fn)=>{timers.push(fn);return timers.length;};
  const copy=p.d.querySelector('[data-preset="copy"]');
  p.w.__dsfResetFired('100');copy.click();
  p.w.__dsfResetFired('101');copy.click();
  timers[0]();copy.click();
  assert.equal(p.calls.filter(c=>c.command==='popup_action').length,2,
   'late A timer must not unlock B and dispatch another B action');
 }finally{p.close();}
});

test('synthetic diagnostics bind late IPC resolution to original ticket without text',async()=>{
 const p=toolbarPage();try{
  let resolveAction;
  p.w.__dsfSelectionTrace=true;
  p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{
   p.calls.push({command,args});
   return command==='popup_action'?new Promise(r=>resolveAction=r):Promise.resolve();
  };
  p.w.__dsfResetFired('200');p.d.querySelector('[data-preset="copy"]').click();
  p.w.__dsfResetFired('201');resolveAction();await tick();
  const result=p.calls.find(c=>c.command==='popup_trace'&&c.args.stage==='resolved');
  assert.equal(result.args.selectionTicket,'200');
  assert.deepEqual(Object.keys(result.args).sort(),['action','selectionTicket','stage']);
 }finally{p.close();}
});


test('resolved more menu permits the immediate settings click without a fixed lockout',async()=>{
 const p=toolbarPage();try{
  let done;
  p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{
   p.calls.push({command,args});
   return command==='popup_action'&&args.preset==='open-more'?new Promise(r=>done=r):Promise.resolve(true);
  };
  p.w.__dsfResetFired('300');
  p.d.querySelector('[data-preset="open-more"]').click();
  done();await tick();
  p.w.__dsfSetPrompts([]);
  p.d.querySelector('[data-preset="open-settings"]').click();
  assert.deepEqual(p.calls.filter(c=>c.command==='popup_action').map(c=>c.args.preset),['open-more','open-settings']);
 }finally{p.close();}
});

test('clicking visible more collapses it and reopening preserves the selection ticket',async()=>{
 const p=toolbarPage();try{
  p.w.__dsfSetPrompts(Array.from({length:6},(_,i)=>({id:`prompt-${i}`,label:`Prompt ${i}`})));
  p.w.__dsfResetFired('350');
  const more=p.d.querySelector('[data-preset="open-more"]');
  more.click();await tick();p.w.__dsfShowMore();
  more.click();await tick();
  assert.deepEqual(p.calls.filter(c=>c.command==='popup_action').map(c=>c.args.preset),
   ['open-more','close-more']);
  p.w.__dsfHideMore();
  more.click();await tick();p.w.__dsfShowMore();
  p.d.querySelector('[data-preset="open-settings"]').click();
  assert.deepEqual(p.calls.filter(c=>c.command==='popup_action').map(c=>[c.args.preset,c.args.selectionTicket]),
   [['open-more','350'],['close-more','350'],['open-more','350'],['open-settings','350']]);
 }finally{p.close();}
});

test('smart selection default and explicit preferences render without a legacy control',async()=>{
 for(const cfg of [{}, {privileged_capture:false}, {privileged_capture:true}]){
  const p=page(c=>c==='config_get'?Promise.resolve(cfg):c==='safe_copy_status'?Promise.resolve({available:false,enabled:false}):undefined);
  try{await tick();
   const el=p.d.querySelector('[data-key="privileged"]');
   assert.equal(el.checked,cfg.privileged_capture!==false);
   assert.match(el.closest('.toggle-row').textContent,/智能划词增强/);
   assert.equal(p.d.querySelector('[data-key="clipboard-fallback"]'),null);
   assert.match(p.d.querySelector('#safe-copy-hint').textContent,/此构建未提供/);
  }finally{p.close();}
 }
});


test('pending more rejects duplicate dispatch and stale completion cannot unlock a new ticket',async()=>{
 const p=toolbarPage();try{
  let done;p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{p.calls.push({command,args});return command==='popup_action'&&args.preset==='open-more'?new Promise(r=>done=r):Promise.resolve(true);};
  p.w.__dsfResetFired('400');const more=p.d.querySelector('[data-preset="open-more"]');more.click();more.click();
  assert.equal(p.calls.filter(c=>c.command==='popup_action').length,1);
  p.w.__dsfResetFired('401');const copy=p.d.querySelector('[data-preset="copy"]');copy.click();done();await tick();copy.click();
  assert.equal(p.calls.filter(c=>c.command==='popup_action').length,2);
 }finally{p.close();}
});

test('failed more IPC can be retried without reusing a consumed action',async()=>{
 const p=toolbarPage();try{
  p.w.console.error=()=>{};
  p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{p.calls.push({command,args});return Promise.reject(Error('native menu failed'));};
  p.w.__dsfResetFired('500');const more=p.d.querySelector('[data-preset="open-more"]');more.click();await tick();more.click();
  assert.equal(p.calls.filter(c=>c.command==='popup_action').length,2);await tick();
 }finally{p.close();}
});

test('late rejected more cannot unlock the replacement menu request',async()=>{
 const p=toolbarPage();try{
  const pending=[];p.w.console.error=()=>{};
  p.w.__TAURI_INTERNALS__.invoke=(command,args)=>{
   p.calls.push({command,args});
   return command==='popup_action'&&args.preset==='open-more'
    ?new Promise((resolve,reject)=>pending.push({resolve,reject})):Promise.resolve(true);
  };
  const prompts=Array.from({length:6},(_,i)=>({id:`prompt-${i}`,label:`Prompt ${i}`}));
  p.w.__dsfSetPrompts(prompts);p.w.__dsfResetFired('600');
  const more=p.d.querySelector('[data-preset="open-more"]');more.click();
  p.w.__dsfSetPrompts(prompts);p.w.__dsfResetFired('601');more.click();
  pending[0].reject(Error('old menu request failed'));await tick();more.click();
  assert.equal(p.calls.filter(c=>c.command==='popup_action').length,2,
   'old rejection must leave the replacement menu request locked');
  pending[1].resolve();await tick();p.w.__dsfShowMore();
  p.d.querySelector('#more-box [data-preset="prompt-5"] span').click();
  const actions=p.calls.filter(c=>c.command==='popup_action');
  assert.deepEqual(actions.map(c=>[c.args.preset,c.args.selectionTicket]),
   [['open-more','600'],['open-more','601'],['prompt-5','601']]);
 }finally{p.close();}
});

test('rapid selection refresh replaces toolbar and overflow before dispatching the latest ticket',()=>{
 const p=toolbarPage();try{
  const bar=p.d.querySelector('#bar'),more=p.d.querySelector('#more-box');
  for(let n=0;n<20;n++){
   const prompts=Array.from({length:n%2?6:2},(_,i)=>({id:`selection-${n}-${i}`,label:`Prompt ${n}/${i}`}));
   p.w.__dsfSetPrompts(prompts);p.w.__dsfResetFired(String(700+n));
   assert.equal(p.d.querySelector('#bar'),bar);assert.equal(p.d.querySelector('#more-box'),more);
   assert.equal(p.d.querySelectorAll('.bar').length,1);
   assert.deepEqual([...bar.querySelectorAll('button')].map(b=>b.dataset.preset),
    ['copy',...prompts.slice(0,4).map(p=>p.id),'open-more']);
   assert.deepEqual([...more.querySelectorAll('button')].map(b=>b.dataset.preset),
    [...prompts.slice(4).map(p=>p.id),'open-settings']);
   assert.equal(p.d.body.classList.contains('menu-mode'),false);
   assert.equal(more.classList.contains('visible'),false);
   p.w.__dsfShowMore();
  }
  p.d.querySelector('#more-box [data-preset="selection-19-5"] span').click();
  p.d.querySelector('#more-box [data-preset="selection-19-5"] span').click();
  const actions=p.calls.filter(c=>c.command==='popup_action');
  assert.equal(actions.length,1);
  assert.deepEqual(JSON.parse(JSON.stringify(actions[0].args)),
   {preset:'selection-19-5',selectionTicket:'719'});
 }finally{p.close();}
});
