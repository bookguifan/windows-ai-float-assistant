import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
const tick=()=>new Promise(r=>setTimeout(r,0));
const nativeStatus=(overrides={})=>({smart_enabled:true,available:true,enabled:false,shortcut:'ctrl-insert',mode:'safe',last_error:null,session_only:true,...overrides});
const granted=(shortcut='ctrl-insert')=>nativeStatus({enabled:true,mode:'compatibility',shortcut});
function page(handler=()=>undefined){
 const dom=new JSDOM(readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8'),{runScripts:'outside-only',url:'https://tauri.localhost'});
 let smart=true;
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{
  calls.push({command,args});const result=handler(command,args);if(result!==undefined)return result;
  if(command==='config_set_privileged')smart=args.enable;
  return Promise.resolve(command==='config_get'?{privileged_capture:smart,clipboard_fallback_enabled:true}:['safe_copy_status','safe_copy_revoke'].includes(command)?nativeStatus({smart_enabled:smart}):true);
 }};dom.window.alert=()=>{};dom.window.confirm=()=>{throw Error('Frontend confirmation must not authorize');};
 for(const s of dom.window.document.querySelectorAll('script'))dom.window.eval(s.textContent);
 dom.window.__dsfSettingsInit({prompts:[]});
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
const fallback=p=>{const el=p.d.querySelector('#safe-copy-enabled');assert.ok(el,'auto copy checkbox exists');return el;};
const shortcut=p=>{const el=p.d.querySelector('#safe-copy-shortcut');assert.ok(el,'granted shortcut selector exists');return el;};
const master=p=>p.d.querySelector('[data-key="privileged"]');
const change=(p,el,value)=>{if(el.type==='checkbox')el.checked=value;else el.value=value;el.dispatchEvent(new p.w.Event('change'));return el;};
const choose=(p,on)=>change(p,fallback(p),on);
const commands=(p,name)=>p.calls.filter(c=>c.command===name);
const plain=value=>JSON.parse(JSON.stringify(value));

test('two switches default to smart on and fallback off with the shortcut hidden',async()=>{
 const p=page();try{await tick();assert.equal(master(p).checked,true);assert.equal(fallback(p).type,'checkbox');assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,false);assert.equal(shortcut(p).hidden,true);assert.equal(shortcut(p).value,'ctrl-insert');assert.deepEqual([...shortcut(p).options].map(o=>o.value),['ctrl-insert','ctrl-c']);assert.equal(p.d.querySelector('#safe-copy-mode'),null);}finally{p.close();}
});
test('fallback requires native consent and revokes without legacy config writes',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve(granted()):undefined);
 try{await tick();choose(p,true);await tick();assert.equal(fallback(p).checked,true);assert.equal(shortcut(p).hidden,false);assert.deepEqual(plain(commands(p,'safe_copy_authorize')[0].args),{mode:'compatibility',shortcut:'ctrl-insert'});
  choose(p,false);await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.deepEqual(plain(commands(p,'safe_copy_revoke')[0].args),{mode:'safe'});assert.equal(commands(p,'config_set_clipboard_fallback').length,0);
 }finally{p.close();}
});
test('legacy config and imported flags cannot grant compatibility consent',async()=>{
 const p=page();try{await tick();assert.equal(fallback(p).checked,false);p.w.__dsfSettingsDataRestored({prompts:[],clipboard_fallback_enabled:true,copy_mode:'compatibility',shortcut:'ctrl-c'});await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.equal(commands(p,'safe_copy_authorize').length,0);assert.equal(commands(p,'config_set_clipboard_fallback').length,0);}finally{p.close();}
});
test('compatibility warning describes shared clipboard risk and supported snapshot limits',async()=>{
 const p=page();try{await tick();const hint=p.d.querySelector('#safe-copy-hint').textContent;for(const pattern of [/延迟渲染.*同步等待/,/不能中断仍未返回/,/不保证.*无损/,/HTML.*RTF.*图片/,/跳过恢复/,/空.*Unicode.*ANSI.*OEM.*区域/,/本次运行/,/一次.*快捷键/,/不.*自动.*另/])assert.match(hint,pattern);}finally{p.close();}
});
test('native denial leaves compatibility off without invoking the legacy setting',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.resolve(nativeStatus()):undefined);
 try{await tick();choose(p,true);await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.equal(commands(p,'safe_copy_authorize').length,1);assert.equal(commands(p,'config_set_clipboard_fallback').length,0);}finally{p.close();}
});
test('pending native consent prevents duplicate requests',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();const el=choose(p,true);assert.equal(el.disabled,true);assert.equal(el.checked,false);choose(p,true);assert.equal(commands(p,'safe_copy_authorize').length,1);done(nativeStatus());await tick();assert.equal(el.disabled,false);assert.equal(el.checked,false);}finally{p.close();}
});
test('native status push cannot cancel or unlock a pending fallback change',async()=>{
 for(const requested of [true,false]){
  let done;const p=page(c=>['safe_copy_authorize','safe_copy_revoke'].includes(c)?new Promise(r=>done=r):undefined);
  try{await tick();if(!requested)p.w.__dsfSafeCopyStatus(granted());const el=choose(p,requested);
   p.w.__dsfSafeCopyStatus(nativeStatus({last_error:'old transaction'}));
   assert.equal(el.disabled,true,'status push must not unlock an outstanding native request');
   choose(p,requested);assert.equal(p.calls.filter(c=>['safe_copy_authorize','safe_copy_revoke'].includes(c.command)).length,1);
   done(requested?granted():nativeStatus());await tick();
   assert.equal(el.checked,requested);assert.equal(el.disabled,false);
  }finally{p.close();}
 }
});
test('authorization errors fail closed with a visible reason and allow retry',async()=>{
 const p=page(c=>c==='safe_copy_authorize'?Promise.reject(Error('native denied')):undefined);
 try{await tick();const el=choose(p,true);await tick();assert.equal(el.checked,false);assert.equal(el.disabled,false);assert.equal(shortcut(p).hidden,true);assert.match(p.d.querySelector('#safe-copy-status').textContent,/native denied/);choose(p,true);await tick();assert.equal(commands(p,'safe_copy_authorize').length,2);}finally{p.close();}
});
test('late status read cannot override a newer native authorization',async()=>{
 let status,delay=false;const p=page(c=>c==='safe_copy_status'?(delay?new Promise(r=>status=r):Promise.resolve(nativeStatus())):c==='safe_copy_authorize'?Promise.resolve(granted()):undefined);
 try{await tick();delay=true;p.w.__dsfSettingsDataRestored({prompts:[]});await tick();const el=choose(p,true);await tick();assert.equal(el.checked,true);status(nativeStatus());await tick();assert.equal(el.checked,true);}finally{p.close();}
});
test('restore reflects runtime revocation rather than imported flags',async()=>{
 let enabled=true;const p=page(c=>c==='safe_copy_status'?Promise.resolve(nativeStatus({enabled,mode:'compatibility'})):undefined);
 try{await tick();const el=fallback(p);assert.equal(el.checked,true);enabled=false;p.w.__dsfSettingsDataRestored({prompts:[]});await tick();await tick();assert.equal(el.checked,false);assert.equal(commands(p,'safe_copy_authorize').length,0);}finally{p.close();}
});
test('both passive native modes render off and revoke without an authorization dialog',async()=>{
 for(const mode of ['off','safe']){const p=page(c=>c==='safe_copy_status'?Promise.resolve(nativeStatus({mode})):undefined);
  try{await tick();assert.equal(fallback(p).checked,false);p.w.__dsfSafeCopyStatus(granted());choose(p,false);await tick();assert.equal(fallback(p).checked,false);assert.equal(commands(p,'safe_copy_revoke').length,1);assert.equal(commands(p,'safe_copy_authorize').length,0);}finally{p.close();}}
});
test('transaction failure is visible without clipboard text',async()=>{
 const p=page();try{await tick();p.w.__dsfSafeCopyStatus({...granted(),last_error:'原剪贴板格式不支持'});assert.match(p.d.querySelector('#safe-copy-status').textContent,/原剪贴板格式不支持/);}finally{p.close();}
});

test('changing the granted shortcut uses one new native consent transaction',async()=>{
 let done;const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();change(p,shortcut(p),'ctrl-c');assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.equal(shortcut(p).disabled,true);assert.deepEqual(plain(commands(p,'safe_copy_authorize')[0].args),{mode:'compatibility',shortcut:'ctrl-c'});assert.equal(commands(p,'safe_copy_revoke').length,0);
  change(p,shortcut(p),'ctrl-insert');choose(p,true);assert.equal(commands(p,'safe_copy_authorize').length,1);done(granted('ctrl-c'));await tick();assert.equal(fallback(p).checked,true);assert.equal(shortcut(p).hidden,false);assert.equal(shortcut(p).value,'ctrl-c');assert.equal(shortcut(p).disabled,false);
 }finally{p.close();}
});
test('canceled shortcut consent revokes the old grant and stale pushes cannot restore it',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_authorize'?Promise.resolve(nativeStatus({shortcut:'ctrl-c'})):undefined);
 try{await tick();change(p,shortcut(p),'ctrl-c');await tick();p.w.__dsfSafeCopyStatus(granted());assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.equal(fallback(p).disabled,false);}finally{p.close();}
});
test('unchanged or hidden shortcut cannot open native consent',async()=>{
 const p=page();try{await tick();change(p,shortcut(p),'ctrl-c');assert.equal(commands(p,'safe_copy_authorize').length,0);p.w.__dsfSafeCopyStatus(granted());change(p,shortcut(p),'ctrl-insert');assert.equal(commands(p,'safe_copy_authorize').length,0);}finally{p.close();}
});
test('shortcut consent rejection fails closed and displays the native error',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_authorize'?Promise.reject(Error('shortcut consent failed')):undefined);
 try{await tick();change(p,shortcut(p),'ctrl-c');await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);assert.equal(commands(p,'safe_copy_revoke').length,1);assert.match(p.d.querySelector('#safe-copy-status').textContent,/shortcut consent failed/);}finally{p.close();}
});
test('smart off revokes fallback immediately, persists once, and refreshes native status',async()=>{
 let enabled=true;const p=page((c,args)=>c==='safe_copy_status'?Promise.resolve(nativeStatus({smart_enabled:enabled,enabled,mode:'compatibility'})):c==='config_set_privileged'?(enabled=args.enable,Promise.resolve(true)):undefined);
 try{await tick();const reads=commands(p,'safe_copy_status').length;master(p).click();assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,true);assert.equal(shortcut(p).hidden,true);await tick();assert.deepEqual(plain(commands(p,'config_set_privileged').map(c=>c.args)),[{enable:false}]);assert.equal(commands(p,'safe_copy_revoke').length,1);assert.ok(commands(p,'safe_copy_status').length>reads);
  fallback(p).disabled=false;choose(p,true);change(p,shortcut(p),'ctrl-c');assert.equal(fallback(p).disabled,true);assert.equal(commands(p,'safe_copy_authorize').length,0);
  p.w.__dsfSafeCopyStatus(granted());assert.equal(master(p).checked,false);assert.equal(fallback(p).checked,false);
 }finally{p.close();}
});
test('late authorization and pushes cannot override master off or unlock duplicate consent',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();choose(p,true);master(p).click();await tick();p.w.__dsfSafeCopyStatus(granted());assert.equal(fallback(p).disabled,true);done(granted());await tick();assert.equal(master(p).checked,false);assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,true);assert.equal(shortcut(p).hidden,true);assert.equal(commands(p,'safe_copy_authorize').length,1);}finally{p.close();}
});
test('off then on during consent still requires a fresh grant after the old modal resolves',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();choose(p,true);master(p).click();await tick();master(p).click();await tick();assert.equal(master(p).checked,true);assert.equal(fallback(p).disabled,true);done(granted());await tick();assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,false);choose(p,true);assert.equal(commands(p,'safe_copy_authorize').length,2);done(nativeStatus());await tick();}finally{p.close();}
});
test('native master-off push invalidates a pending authorization without unlocking it',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();choose(p,true);p.w.__dsfSafeCopyStatus(nativeStatus({smart_enabled:false}));assert.equal(master(p).checked,false);assert.equal(fallback(p).disabled,true);done(granted());await tick();assert.equal(master(p).checked,false);assert.equal(fallback(p).checked,false);}finally{p.close();}
});
test('turning smart back on refreshes status but never restores an old fallback grant',async()=>{
 let smart=false;const p=page((c,args)=>c==='config_get'?Promise.resolve({privileged_capture:smart}):c==='safe_copy_status'?Promise.resolve(nativeStatus({smart_enabled:smart,enabled:true,mode:'compatibility'})):c==='config_set_privileged'?(smart=args.enable,Promise.resolve(true)):undefined);
 try{await tick();assert.equal(master(p).checked,false);assert.equal(fallback(p).checked,false);const reads=commands(p,'safe_copy_status').length;master(p).click();await tick();assert.equal(master(p).checked,true);assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,false);assert.ok(commands(p,'safe_copy_status').length>reads);assert.equal(commands(p,'safe_copy_authorize').length,0);}finally{p.close();}
});
test('failed master persistence rolls back master, keeps fallback revoked, and exposes the error',async()=>{
 for(const failure of [false,Error('disk failed')]){const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='config_set_privileged'?(failure instanceof Error?Promise.reject(failure):Promise.resolve(failure)):undefined);
  try{await tick();p.w.console.error=()=>{};master(p).click();await tick();assert.equal(master(p).checked,true);assert.equal(master(p).disabled,false);assert.equal(fallback(p).checked,false);assert.match(p.d.querySelector('#safe-copy-status').textContent,/保存失败|disk failed/);}finally{p.close();}}
});
test('revoke failure stays off even after stale enabled status and permits a new consent',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_revoke'?Promise.reject(Error('revoke failed')):c==='safe_copy_authorize'?Promise.resolve(granted()):undefined);
 try{await tick();choose(p,false);await tick();p.w.__dsfSafeCopyStatus(granted());assert.equal(fallback(p).checked,false);assert.match(p.d.querySelector('#safe-copy-status').textContent,/revoke failed/);choose(p,true);await tick();assert.equal(fallback(p).checked,true);}finally{p.close();}
});
test('status read failure disables fallback and exposes a reason',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.reject(Error('status unavailable')):undefined);
 try{await tick();assert.equal(fallback(p).checked,false);assert.equal(fallback(p).disabled,true);assert.equal(shortcut(p).hidden,true);assert.match(p.d.querySelector('#safe-copy-status').textContent,/status unavailable/);}finally{p.close();}
});
test('late enabled read cannot undo a newer native revocation push',async()=>{
 const reads=[];const p=page(c=>c==='safe_copy_status'?new Promise(r=>reads.push(r)):undefined);
 try{await tick();p.w.__dsfSafeCopyStatus(nativeStatus({smart_enabled:false}));reads.forEach(r=>r(granted()));await tick();assert.equal(master(p).checked,false);assert.equal(fallback(p).checked,false);}finally{p.close();}
});
test('restore invalidates pending consent and never grants from imported flags',async()=>{
 let done;const p=page(c=>c==='safe_copy_authorize'?new Promise(r=>done=r):undefined);
 try{await tick();choose(p,true);p.w.__dsfSettingsDataRestored({prompts:[],privileged_capture:true,clipboard_fallback_enabled:true});await tick();done(granted());await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);}finally{p.close();}
});

test('runtime revocation cannot be undone by a delayed enabled push',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):undefined);
 try{await tick();p.w.__dsfSafeCopyStatus(nativeStatus());p.w.__dsfSafeCopyStatus(granted());assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);}finally{p.close();}
});
test('a native master-off notification does not become a permanent user preference',async()=>{
 const p=page();try{
  await tick();master(p).click();await tick();master(p).click();await tick();
  assert.equal(master(p).checked,true);
  p.w.__dsfSafeCopyStatus(nativeStatus({smart_enabled:false}));
  assert.equal(fallback(p).checked,false);
  // A fresh native snapshot must be able to reflect the persisted master-on choice.
  p.w.__dsfSafeCopyStatus(nativeStatus({smart_enabled:true}));
  assert.equal(master(p).checked,true);
  assert.equal(fallback(p).checked,false);
 }finally{p.close();}
});
test('old shortcut status cannot replace a newly authorized shortcut',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_authorize'?Promise.resolve(granted('ctrl-c')):undefined);
 try{await tick();change(p,shortcut(p),'ctrl-c');await tick();p.w.__dsfSafeCopyStatus(granted());assert.equal(fallback(p).checked,true);assert.equal(shortcut(p).value,'ctrl-c');}finally{p.close();}
});
test('authorization response for a different shortcut cannot grant the requested shortcut',async()=>{
 const p=page(c=>c==='safe_copy_status'?Promise.resolve(granted()):c==='safe_copy_authorize'?Promise.resolve(granted()):undefined);
 try{await tick();change(p,shortcut(p),'ctrl-c');await tick();assert.equal(fallback(p).checked,false);assert.equal(shortcut(p).hidden,true);}finally{p.close();}
});
test('native master state takes precedence over a conflicting late config snapshot',async()=>{
 const reads=[];const p=page(c=>c==='config_get'?new Promise(r=>reads.push(r)):c==='safe_copy_status'?Promise.resolve(nativeStatus({smart_enabled:false,enabled:true,mode:'compatibility'})):undefined);
 try{await tick();reads.forEach(r=>r({privileged_capture:true,clipboard_fallback_enabled:true}));await tick();assert.equal(master(p).checked,false);assert.equal(fallback(p).disabled,true);assert.equal(fallback(p).checked,false);}finally{p.close();}
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
 for(const cfg of [{},{clipboard_fallback_enabled:false},{clipboard_fallback_enabled:true}]){const p=page(c=>c==='config_get'?Promise.resolve(cfg):c==='safe_copy_status'?Promise.resolve(nativeStatus({available:false,enabled:true,mode:'compatibility'})):undefined);
  try{await tick();const el=fallback(p);assert.equal(el.disabled,true);assert.equal(el.checked,false);p.w.__dsfSettingsDataRestored({prompts:[],clipboard_fallback_enabled:true});await tick();await tick();assert.equal(el.disabled,true);el.disabled=false;choose(p,true);await tick();assert.equal(el.disabled,true);assert.equal(el.checked,false);assert.equal(commands(p,'safe_copy_authorize').length,0);assert.equal(commands(p,'config_set_clipboard_fallback').length,0);assert.match(p.d.querySelector('#safe-copy-hint').textContent,/此构建未提供/);}finally{p.close();}}
});

test('delayed legacy config cannot reenable an unavailable fallback',async()=>{
 const reads=[];const p=page(c=>c==='config_get'?new Promise(r=>reads.push(r)):c==='safe_copy_status'?Promise.resolve(nativeStatus({available:false})):undefined);
 try{await tick();fallback(p).disabled=false;choose(p,true);reads.forEach(r=>r({clipboard_fallback_enabled:true}));await tick();assert.equal(fallback(p).disabled,true);assert.equal(fallback(p).checked,false);assert.equal(commands(p,'safe_copy_authorize').length,0);}finally{p.close();}
});

test('pending master save rejects duplicates and stale config cannot overwrite the saved choice',async()=>{
 let save,delay=false;const reads=[];const p=page(c=>c==='config_get'?(delay?new Promise(r=>reads.push(r)):Promise.resolve({privileged_capture:true})):c==='config_set_privileged'?new Promise(r=>save=r):undefined);
 try{await tick();delay=true;p.w.__dsfSettingsDataRestored({prompts:[]});await tick();master(p).click();assert.equal(master(p).disabled,true);master(p).click();assert.equal(commands(p,'config_set_privileged').length,1);save(true);await tick();reads.forEach(r=>r({privileged_capture:true}));await tick();assert.equal(master(p).checked,false);assert.equal(master(p).disabled,false);assert.equal(fallback(p).disabled,true);}finally{p.close();}
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
  const p=page(c=>c==='config_get'?Promise.resolve(cfg):c==='safe_copy_status'?Promise.resolve(nativeStatus({smart_enabled:cfg.privileged_capture!==false,available:false})):undefined);
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
