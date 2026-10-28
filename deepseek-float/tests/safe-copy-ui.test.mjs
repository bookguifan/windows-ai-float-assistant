import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
const tick=()=>new Promise(r=>setTimeout(r,0));
function page(handler=()=>undefined){
 const dom=new JSDOM(readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8'),{runScripts:'outside-only',url:'https://tauri.localhost'});
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{
  calls.push({command,args});return handler(command,args)??Promise.resolve(command==='config_get'?{clipboard_fallback_enabled:true}:command==='safe_copy_status'?{available:true,enabled:false}:true);
 }};dom.window.alert=()=>{};dom.window.confirm=()=>{throw Error('Frontend confirmation must not authorize');};
 for(const s of dom.window.document.querySelectorAll('script'))dom.window.eval(s.textContent);
 dom.window.__dsfSettingsInit({prompts:[]});
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
test('legacy config cannot grant new safe copy authorization',async()=>{
 const p=page();try{await tick();assert.equal(p.d.querySelector('[data-key="safe-copy"]').checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
});
test('native denial leaves the feature off and never invokes legacy enabling',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve({available:true,enabled:false}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);assert.equal(p.calls.filter(c=>c.command==='config_set_clipboard_fallback').length,0);}finally{p.close();}
});
test('native authorization enables and explicit disable revokes without a frontend prompt',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve({available:true,enabled:true}):c==='safe_copy_revoke'?Promise.resolve({available:true,enabled:false}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,true);el.click();await tick();assert.equal(el.checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_revoke').length,1);}finally{p.close();}
});
test('pending consent prevents duplicate user requests',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();assert.equal(el.disabled,true);el.click();assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);done({available:true,enabled:false});await tick();assert.equal(el.disabled,false);assert.equal(el.checked,false);}finally{p.close();}
});
test('authorization errors fail closed and unlock the control for retry',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.reject(Error('denied')):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,false);assert.equal(el.disabled,false);}finally{p.close();}
});

test('late status read cannot override a newer explicit native authorization',async()=>{
 let status,reads=0;const p=page(c=>c==='safe_copy_status'?(++reads===1?Promise.resolve({available:true,enabled:false}):new Promise(r=>status=r)):c==='safe_copy_authorize'?Promise.resolve({available:true,enabled:true}):undefined);
 try{await tick();p.w.__dsfSettingsDataRestored({prompts:[]});await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,true);status({available:true,enabled:false});await tick();assert.equal(el.checked,true);}finally{p.close();}
});
test('restore refresh uses runtime consent state rather than imported feature flags',async()=>{
 let enabled=true;const p=page(c=>c==='safe_copy_status'?Promise.resolve({available:true,enabled}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');assert.equal(el.checked,true);enabled=false;p.w.__dsfSettingsDataRestored({prompts:[]});await tick();await tick();assert.equal(el.checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
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

test('distribution capability stays disabled after restore even if settings request true',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve({available:false,enabled:true}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');assert.equal(el.disabled,true);assert.equal(el.checked,false);
 for(let n=0;n<2;n++){p.w.__dsfSettingsDataRestored({prompts:[],clipboard_fallback_enabled:true});await tick();await tick();assert.equal(el.disabled,true);assert.equal(el.checked,false);}
 el.click();await tick();assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);
 assert.match(p.d.querySelector('#safe-copy-hint').textContent,/本发行构建已禁用/);
 }finally{p.close();}
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

test('smart selection default and explicit preferences render without a legacy control',async()=>{
 for(const cfg of [{}, {privileged_capture:false}, {privileged_capture:true}]){
  const p=page(c=>c==='config_get'?Promise.resolve(cfg):c==='safe_copy_status'?Promise.resolve({available:false,enabled:false}):undefined);
  try{await tick();
   const el=p.d.querySelector('[data-key="privileged"]');
   assert.equal(el.checked,cfg.privileged_capture!==false);
   assert.match(el.closest('.toggle-row').textContent,/智能划词增强/);
   assert.equal(p.d.querySelector('[data-key="clipboard-fallback"]'),null);
   assert.match(p.d.querySelector('#safe-copy-hint').textContent,/待安全验收/);
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
