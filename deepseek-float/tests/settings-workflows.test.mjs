import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';

const initial = [{id:'explain',label:'解释',instruction:'解释内容',is_builtin:true,hidden:false,show_card:true}];
const tick = () => new Promise(resolve => setTimeout(resolve, 0));
function page(handler = () => undefined) {
  const html = readFileSync(new URL('../poc/settings.html', import.meta.url), 'utf8');
  const dom = new JSDOM(html, {runScripts:'outside-only', url:'https://tauri.localhost'});
  const calls = [];
  dom.window.__TAURI_INTERNALS__ = {invoke:(command,args) => {
    calls.push({command,args});
    const result = handler(command,args);
    return result !== undefined ? result : Promise.resolve(command === 'config_get' ? {} : command === 'settings_save' ? args.prompts : true);
  }};
  dom.window.alert = () => {};
  dom.window.confirm = () => true;
  for (const script of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)) dom.window.eval(script[1]);
  dom.window.__dsfSettingsInit({prompts:initial});
  const d = dom.window.document;
  return {w:dom.window,d,calls,close:()=>dom.window.close()};
}

test('prompt save failure is visible and retry preserves the edited snapshot', async () => {
  let fails = true;
  const p = page((c,a) => c === 'settings_save' ? (fails ? Promise.reject('磁盘写入失败') : Promise.resolve(a.prompts)) : undefined);
  try {
    p.d.querySelector('[data-act="edit"]').click();
    p.d.getElementById('f-label').value = '我的解释';
    p.d.getElementById('f-save').click(); await tick();
    assert.match(p.d.getElementById('prompt-save-status')?.textContent || '', /保存失败.*磁盘写入失败/);
    p.w.__dsfSettingsInit({prompts:initial}); await tick();
    assert.match(p.d.getElementById('list').textContent, /我的解释/);
    fails = false;
    p.d.getElementById('prompt-save-retry').click(); await tick();
    const saves = p.calls.filter(c => c.command === 'settings_save');
    assert.equal(saves.length, 2);
    assert.equal(saves[1].args.prompts[0].label, '我的解释');
    assert.match(p.d.getElementById('prompt-save-status').textContent, /已保存/);
    assert.equal(p.d.getElementById('prompt-save-retry').hidden, true);
  } finally { p.close(); }
});

test('rapid edits serialize writes and do not mark newer changes as saved early', async () => {
  const pending = [];
  const p = page(c => c === 'settings_save' ? new Promise((resolve,reject) => pending.push({resolve,reject})) : undefined);
  try {
    p.d.querySelector('[data-act="hide"]').click();
    p.d.querySelector('[data-act="card"]').click();
    assert.equal(pending.length, 1, 'second write must wait for the first');
    pending[0].resolve(initial); await tick();
    assert.equal(pending.length, 2);
    assert.match(p.d.getElementById('prompt-save-status').textContent, /保存中/);
    const latest = p.calls.filter(c => c.command === 'settings_save')[1].args.prompts;
    assert.equal(latest[0].hidden, true); assert.equal(latest[0].show_card, false);
    pending[1].resolve(latest); await tick();
    assert.match(p.d.getElementById('prompt-save-status').textContent, /已保存/);
  } finally { p.close(); }
});

test('successful save renders the backend normalized list', async () => {
  const p = page(c => c === 'settings_save' ? Promise.resolve([{...initial[0],label:'规范化名称',hidden:true}]) : undefined);
  try {
    p.d.querySelector('[data-act="hide"]').click(); await tick();
    assert.match(p.d.getElementById('list').textContent, /规范化名称/);
  } finally { p.close(); }
});

test('custom sites can be added, reordered, renamed, switched and removed independently', async () => {
  const p = page((c,a) => c === 'panel_sites_save' ? Promise.resolve(a.sites) : undefined);
  const add = async (label,url) => {
    p.d.getElementById('site-label').value = label;
    p.d.getElementById('site-url').value = url;
    p.d.getElementById('site-save').click(); await tick();
  };
  try {
    await tick();
    assert.ok(p.d.getElementById('site-save'), 'site manager is available');
    await add('站点甲','https://a.example/chat');
    await add('站点乙','https://b.example/chat');
    assert.equal(p.calls.filter(c => c.command === 'config_set_panel_url').length, 0);
    p.d.querySelectorAll('[data-site-action="up"]')[1].click(); await tick();
    p.d.querySelector('[data-site-action="edit"]').click();
    p.d.getElementById('site-label').value = '重命名乙';
    p.d.getElementById('site-save').click(); await tick();
    assert.match(p.d.getElementById('custom-sites-list').textContent, /重命名乙/);
    p.d.querySelector('[data-site-action="switch"]').click(); await tick();
    assert.equal(p.calls.find(c => c.command === 'config_set_panel_url').args.url, 'https://b.example/chat');
    p.d.querySelector('[data-site-action="delete"]').click(); await tick();
    const saves = p.calls.filter(c => c.command === 'panel_sites_save');
    assert.deepEqual(Array.from(saves.at(-1).args.sites, s => s.url), ['https://a.example/chat']);
  } finally { p.close(); }
});

test('failed site save retains local changes for retry and blocks stale reinitialization', async () => {
  let fails = true;
  const p = page((c,a) => c === 'panel_sites_save' ? (fails ? Promise.reject('站点目录只读') : Promise.resolve(a.sites)) : undefined);
  try {
    await tick();
    assert.ok(p.d.getElementById('site-save'));
    p.d.getElementById('site-label').value = '保留站点';
    p.d.getElementById('site-url').value = 'https://keep.example/';
    p.d.getElementById('site-save').click(); await tick();
    assert.match(p.d.getElementById('sites-save-status').textContent, /站点目录只读/);
    p.w.__dsfSettingsInit({prompts:initial}); await tick();
    assert.match(p.d.getElementById('custom-sites-list').textContent, /保留站点/);
    fails = false; p.d.getElementById('sites-save-retry').click(); await tick();
    assert.equal(p.calls.filter(c => c.command === 'panel_sites_save').at(-1).args.sites[0].label, '保留站点');
    assert.match(p.d.getElementById('sites-save-status').textContent, /已保存/);
  } finally { p.close(); }
});

test('site changes wait for the first baseline so existing saved sites are not overwritten', async () => {
  let resolveOldConfig;
  let firstConfig = true;
  const p = page((c,a) => {
    if (c === 'config_get' && firstConfig) { firstConfig = false; return new Promise(resolve => {resolveOldConfig = resolve;}); }
    if (c === 'panel_sites_save') return Promise.resolve(a.sites);
  });
  try {
    p.d.getElementById('site-label').value = '新保存'; p.d.getElementById('site-url').value = 'https://new.example/';
    assert.equal(p.d.getElementById('site-save').disabled, true);
    p.d.getElementById('site-save').click(); await tick();
    assert.equal(p.calls.filter(c => c.command === 'panel_sites_save').length, 0);
    resolveOldConfig({custom_sites:[{label:'已有站点',url:'https://existing.example/'}]}); await tick();
    p.d.getElementById('site-save').click(); await tick();
    assert.match(p.d.getElementById('custom-sites-list').textContent, /新保存/);
    assert.match(p.d.getElementById('custom-sites-list').textContent, /已有站点/);
    assert.equal(p.calls.filter(c => c.command === 'panel_sites_save').at(-1).args.sites.length, 2);
  } finally { p.close(); }
});

test('a delayed refresh cannot replace custom sites saved after the refresh started', async () => {
  let resolveRefresh;
  let defer = false;
  const p = page((c,a) => {
    if (c === 'config_get' && defer && !resolveRefresh) return new Promise(resolve => {resolveRefresh = resolve;});
    if (c === 'panel_sites_save') return Promise.resolve(a.sites);
  });
  try {
    await tick(); defer = true; p.w.__dsfSettingsInit({prompts:initial});
    p.d.getElementById('site-label').value = '新保存'; p.d.getElementById('site-url').value = 'https://new.example/';
    p.d.getElementById('site-save').click(); await tick();
    resolveRefresh({custom_sites:[]}); await tick();
    assert.match(p.d.getElementById('custom-sites-list').textContent, /新保存/);
  } finally { p.close(); }
});

test('failed initial site read blocks saving and offers a separate read retry', async () => {
  let fails = true;
  const p = page((c) => c === 'config_get' ? (fails ? Promise.reject('无法读取配置') : Promise.resolve({custom_sites:[{label:'已有站点',url:'https://existing.example/'}]})) : undefined);
  try {
    await tick(); assert.equal(p.d.getElementById('site-save').disabled, true);
    assert.match(p.d.getElementById('sites-save-status').textContent, /读取.*失败/);
    assert.equal(p.d.getElementById('sites-load-retry').hidden, false);
    fails = false; p.d.getElementById('sites-load-retry').click(); await tick();
    assert.equal(p.d.getElementById('site-save').disabled, false);
    assert.match(p.d.getElementById('custom-sites-list').textContent, /已有站点/);
  } finally { p.close(); }
});

test('the newest prompt edit is saved even if an older in-flight snapshot fails', async () => {
  const pending = [];
  const p = page(c => c === 'settings_save' ? new Promise((resolve,reject) => pending.push({resolve,reject})) : undefined);
  try {
    p.d.querySelector('[data-act="hide"]').click(); p.d.querySelector('[data-act="card"]').click();
    pending[0].reject('旧保存失败'); await tick(); assert.equal(pending.length, 2);
    const latest = p.calls.filter(c => c.command === 'settings_save')[1].args.prompts;
    pending[1].resolve(latest); await tick();
    assert.match(p.d.getElementById('prompt-save-status').textContent, /已保存/);
    assert.equal(p.d.getElementById('prompt-save-retry').hidden, true);
  } finally { p.close(); }
});


test('release capability locks fallback off even for legacy enabled config and forged change events', async () => {
  const p = page(c => c === 'config_get' ? Promise.resolve({clipboard_fallback_enabled:true}) : undefined);
  try {
    await tick();
    const box = p.d.querySelector('[data-key="safe-copy"]');
    assert.equal(box.checked, false);
    assert.equal(box.disabled, true);
    box.disabled = false;
    box.checked = true;
    box.dispatchEvent(new p.w.Event('change'));
    await tick();
    assert.equal(box.checked, false);
    assert.equal(box.disabled, true);
    assert.equal(p.calls.filter(c => c.command === 'config_set_clipboard_fallback').length, 0);
  } finally { p.close(); }
});

test('experimental control stays disabled for missing, false and legacy true settings', async () => {
  for (const cfg of [{}, {clipboard_fallback_enabled:false}, {clipboard_fallback_enabled:true}]) {
    const p = page(c => c === 'config_get' ? Promise.resolve(cfg) : undefined);
    try {
      await tick();
      const box = p.d.querySelector('[data-key="safe-copy"]');
      assert.ok(box, 'experimental capability entry exists');
      assert.equal(box.checked, false);
      assert.equal(box.disabled, true);
      assert.match(box.closest('.toggle-row').textContent, /待安全验收/);
      box.checked = !box.checked;
      box.dispatchEvent(new p.w.Event('change'));
      await tick();
      assert.equal(box.checked, false);
      assert.equal(p.calls.filter(c => c.command === 'config_set_clipboard_fallback').length, 0);
    } finally { p.close(); }
  }
});

test('disabled fallback never invokes persistence even when the DOM disabled flag is removed', async () => {
  const p = page(c => c === 'config_set_clipboard_fallback' ? Promise.reject('must not be invoked') : undefined);
  try {
    await tick();
    const box = p.d.querySelector('[data-key="safe-copy"]');
    assert.ok(box);
    box.disabled = false;
    box.checked = true; box.dispatchEvent(new p.w.Event('change'));
    assert.equal(box.disabled, true);
    await tick();
    assert.equal(box.checked, false);
    assert.equal(box.disabled, true);
    assert.equal(p.calls.filter(c => c.command === 'config_set_clipboard_fallback').length, 0);
  } finally { p.close(); }
});


test('delayed legacy config cannot reenable the public fallback control', async () => {
  let resolve;
  const p = page(c => c === 'config_get' ? new Promise(r => {resolve=r;}) : undefined);
  try {
    const box = p.d.querySelector('[data-key="safe-copy"]');
    box.checked = true; box.dispatchEvent(new p.w.Event('change')); await tick();
    resolve({clipboard_fallback_enabled:true}); await tick();
    assert.equal(box.checked, false);
    assert.equal(box.disabled, true);
    assert.equal(p.calls.filter(c => c.command === 'config_set_clipboard_fallback').length, 0);
  } finally { p.close(); }
});

test('smart selection off persists and restore refresh respects the explicit choice',async()=>{
 let enabled=true;
 const p=page((c,args)=>c==='config_get'?Promise.resolve({privileged_capture:enabled}):c==='config_set_privileged'?(enabled=args.enable,Promise.resolve(true)):undefined);
 try{await tick();const box=p.d.querySelector('[data-key="privileged"]');assert.equal(box.checked,true);box.click();await tick();
  assert.equal(enabled,false);assert.equal(p.calls.filter(c=>c.command==='config_set_privileged').length,1);
  p.w.__dsfSettingsDataRestored({prompts:initial});await tick();await tick();assert.equal(box.checked,false);
  assert.equal(p.d.querySelector('[data-key="clipboard-fallback"]'),null);
 }finally{p.close();}
});

test('smart selection save denial rolls the displayed preference back',async()=>{
 const p=page(c=>c==='config_get'?Promise.resolve({privileged_capture:true}):c==='config_set_privileged'?Promise.resolve(false):undefined);
 try{await tick();const box=p.d.querySelector('[data-key="privileged"]');box.click();await tick();assert.equal(box.checked,true);}finally{p.close();}
});


test('pending smart selection save prevents duplicate preference writes', async () => {
  let finish;
  const p = page(c => c === 'config_get' ? Promise.resolve({privileged_capture:true}) : c === 'config_set_privileged' ? new Promise(r => {finish=r;}) : undefined);
  try {
    await tick(); const box=p.d.querySelector('[data-key="privileged"]');
    box.click(); assert.equal(box.disabled,true);
    box.click(); assert.equal(p.calls.filter(c=>c.command==='config_set_privileged').length,1);
    finish(true); await tick(); assert.equal(box.disabled,false); assert.equal(box.checked,false);
  } finally {p.close();}
});

test('late config read cannot overwrite a newer saved smart selection preference', async () => {
  const pendingReads=[]; let stale=false;
  const p=page(c=>c==='config_get' ? (stale ? new Promise(r=>{pendingReads.push(r);}) : Promise.resolve({privileged_capture:true})) : undefined);
  try {
    await tick(); stale=true; p.w.__dsfSettingsDataRestored({prompts:initial}); await tick();
    assert.ok(pendingReads.length>0);
    const box=p.d.querySelector('[data-key="privileged"]'); box.click(); await tick();
    assert.equal(box.checked,false); pendingReads.forEach(resolve=>resolve({privileged_capture:true})); await tick();
    assert.equal(box.checked,false);
  } finally {p.close();}
});
