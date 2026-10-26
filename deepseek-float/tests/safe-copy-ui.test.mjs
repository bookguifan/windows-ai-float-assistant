import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
const tick=()=>new Promise(r=>setTimeout(r,0));
function page(handler=()=>undefined){
 const dom=new JSDOM(readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8'),{runScripts:'outside-only',url:'https://tauri.localhost'});
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{
  calls.push({command,args});return handler(command,args)??Promise.resolve(command==='config_get'?{clipboard_fallback_enabled:true}:command==='safe_copy_status'?{enabled:false}:true);
 }};dom.window.alert=()=>{};dom.window.confirm=()=>{throw Error('Frontend confirmation must not authorize');};
 for(const s of dom.window.document.querySelectorAll('script'))dom.window.eval(s.textContent);
 dom.window.__dsfSettingsInit({prompts:[]});
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
test('legacy config cannot grant new safe copy authorization',async()=>{
 const p=page();try{await tick();assert.equal(p.d.querySelector('[data-key="safe-copy"]').checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,0);}finally{p.close();}
});
test('native denial leaves the feature off and never invokes legacy enabling',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve({enabled:false}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);assert.equal(p.calls.filter(c=>c.command==='config_set_clipboard_fallback').length,0);}finally{p.close();}
});
test('native authorization enables and explicit disable revokes without a frontend prompt',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve({enabled:true}):c==='safe_copy_revoke'?Promise.resolve({enabled:false}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,true);el.click();await tick();assert.equal(el.checked,false);assert.equal(p.calls.filter(c=>c.command==='safe_copy_revoke').length,1);}finally{p.close();}
});
test('pending consent prevents duplicate user requests',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();assert.equal(el.disabled,true);el.click();assert.equal(p.calls.filter(c=>c.command==='safe_copy_authorize').length,1);done({enabled:false});await tick();assert.equal(el.disabled,false);assert.equal(el.checked,false);}finally{p.close();}
});
test('authorization errors fail closed and unlock the control for retry',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.reject(Error('denied')):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,false);assert.equal(el.disabled,false);}finally{p.close();}
});

test('late status read cannot override a newer explicit native authorization',async()=>{
 let status;const p=page(c=>c==='safe_copy_status'?new Promise(r=>status=r):c==='safe_copy_authorize'?Promise.resolve({enabled:true}):undefined);
 try{await tick();const el=p.d.querySelector('[data-key="safe-copy"]');el.click();await tick();assert.equal(el.checked,true);status({enabled:false});await tick();assert.equal(el.checked,true);}finally{p.close();}
});
test('restore refresh uses runtime consent state rather than imported feature flags',async()=>{
 let enabled=true;const p=page(c=>c==='safe_copy_status'?Promise.resolve({enabled}):undefined);
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
