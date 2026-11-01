import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';

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
  dom.window.__dsfSettingsInit({prompts:[{id:'explain',label:'解释',instruction:'解释',is_builtin:true,hidden:false,show_card:true}]});
  return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}

const preview = {
  ticket: 7,
  file_name: 'deepseek-float-backup-20261018-120000.json',
  created_at: '2026-10-18T12:00:00Z',
  app_version: '0.1.0',
  current_site: 'https://work.example/chat',
  counts: {prompts:5,custom_sites:2,favorites:7,settings:4},
  conflict_count: 2,
  conflict_details: ['提示词「总结」存在冲突，合并时保留本机版本', '收藏 ID 42 内容不同，合并时保留本机版本'],
  conflicts_truncated: false,
};

test('backup export reports the saved file and data counts', async () => {
  const p = page(c => c === 'backup_export' ? Promise.resolve({
    file_name:'deepseek-float-backup-20261018-120000.json',
    path:'C:\\Users\\tester\\Documents\\deepseek-float-backup-20261018-120000.json',
    counts:{prompts:5,custom_sites:2,favorites:7,settings:4},
  }) : undefined);
  try {
    p.d.getElementById('backup-export').click();
    await tick();
    const status = p.d.getElementById('backup-status').textContent;
    assert.match(status, /deepseek-float-backup-20261018-120000\.json/);
    assert.match(status, /收藏 7 条/);
  } finally { p.close(); }
});

test('import preview lists counts and conflict rules before restore', async () => {
  const p = page(c => c === 'backup_import_preview' ? Promise.resolve(preview) : undefined);
  try {
    p.d.getElementById('backup-import').click();
    await tick();
    assert.equal(p.d.getElementById('backup-preview').hidden, false);
    assert.match(p.d.getElementById('backup-preview-counts').textContent, /提示词：5 条.*自定义站点：2 个.*收藏：7 条/);
    assert.match(p.d.getElementById('backup-conflicts').textContent, /保留本机版本/);
    p.d.getElementById('backup-cancel').click();
    await tick();
    assert.ok(p.calls.some(c => c.command === 'backup_import_cancel' && c.args.ticket === 7));
    assert.equal(p.d.getElementById('backup-preview').hidden, true);
  } finally { p.close(); }
});

test('merge restore sends the selected ticket and reports the result', async () => {
  const p = page(c => c === 'backup_import_preview' ? Promise.resolve(preview) : c === 'backup_restore' ? Promise.resolve({
    mode:'merge', prompts:1, sites:1, favorites:4, settings:1, conflicts:2, restart_required:true,
  }) : undefined);
  try {
    p.d.getElementById('backup-import').click(); await tick();
    p.d.getElementById('backup-restore-confirm').click(); await tick();
    const restore = p.calls.find(c => c.command === 'backup_restore');
    assert.deepEqual({...restore.args}, {ticket:7, mode:'merge', confirmOverwrite:false});
    assert.match(p.d.getElementById('backup-status').textContent, /恢复完成/);
    assert.match(p.d.getElementById('backup-status').textContent, /收藏.*4/);
    assert.equal(p.d.getElementById('backup-preview').hidden, true);
  } finally { p.close(); }
});

test('overwrite requires a second confirmation and duplicate restore clicks are blocked', async () => {
  let release;
  const p = page(c => c === 'backup_import_preview' ? Promise.resolve(preview) : c === 'backup_restore' ? new Promise(resolve => { release = resolve; }) : undefined);
  try {
    let confirmations = 0;
    p.w.confirm = () => { confirmations++; return true; };
    p.d.getElementById('backup-import').click(); await tick();
    p.d.querySelector('input[name="backup-mode"][value="overwrite"]').checked = true;
    p.d.getElementById('backup-restore-confirm').click();
    assert.equal(confirmations, 1);
    assert.equal(p.d.getElementById('backup-restore-confirm').disabled, true);
    p.d.getElementById('backup-restore-confirm').click();
    assert.equal(p.calls.filter(c => c.command === 'backup_restore').length, 1);
    const restore = p.calls.find(c => c.command === 'backup_restore');
    assert.deepEqual({...restore.args}, {ticket:7, mode:'overwrite', confirmOverwrite:true});
    release({mode:'overwrite',prompts:5,sites:2,favorites:7,settings:4,conflicts:0,restart_required:true});
    await tick();
    assert.equal(p.d.getElementById('backup-preview').hidden, true);
  } finally { p.close(); }
});

test('cancelled overwrite confirmation never invokes restore', async () => {
  const p = page(c => c === 'backup_import_preview' ? Promise.resolve(preview) : undefined);
  try {
    p.w.confirm = () => false;
    p.d.getElementById('backup-import').click(); await tick();
    p.d.querySelector('input[name="backup-mode"][value="overwrite"]').checked = true;
    p.d.getElementById('backup-restore-confirm').click(); await tick();
    assert.equal(p.calls.filter(c => c.command === 'backup_restore').length, 0);
    assert.equal(p.d.getElementById('backup-preview').hidden, false);
  } finally { p.close(); }
});

test('failed import shows a useful error and allows another attempt', async () => {
  const p = page(c => c === 'backup_import_preview' ? Promise.reject('备份文件格式无效') : undefined);
  try {
    p.d.getElementById('backup-import').click(); await tick();
    assert.match(p.d.getElementById('backup-status').textContent, /备份文件格式无效/);
    assert.equal(p.d.getElementById('backup-import').disabled, false);
    assert.equal(p.d.getElementById('backup-preview').hidden, true);
  } finally { p.close(); }
});

test('unsaved prompt edits block backup instead of exporting a stale snapshot', async () => {
  const p = page(c => c === 'settings_save' ? new Promise(() => {}) : undefined);
  try {
    p.d.querySelector('[data-act="edit"]').click();
    p.d.getElementById('f-label').value = '尚未保存';
    p.d.getElementById('f-save').click();
    await tick();
    p.d.getElementById('backup-export').click();
    await tick();
    assert.equal(p.calls.filter(c => c.command === 'backup_export').length, 0);
    assert.match(p.d.getElementById('backup-status').textContent, /先保存或取消/);
  } finally { p.close(); }
});

test('cancelled file picker leaves data untouched and reports cancellation', async () => {
  const p = page(c => c === 'backup_import_preview' ? Promise.resolve(null) : undefined);
  try {
    p.d.getElementById('backup-import').click();
    await tick();
    assert.equal(p.d.getElementById('backup-preview').hidden, true);
    assert.match(p.d.getElementById('backup-status').textContent, /取消导入/);
    assert.equal(p.calls.filter(c => c.command === 'backup_restore').length, 0);
  } finally { p.close(); }
});

test('successful restore refreshes prompt, site, and shortcut controls in place', async () => {
  const restoredPrompts = [
    {id:'explain',label:'迁移后的解释',instruction:'解释',is_builtin:true,hidden:false,show_card:true},
    {id:'translate',label:'翻译',instruction:'翻译',is_builtin:true,hidden:false,show_card:true},
    {id:'summarize',label:'总结',instruction:'总结',is_builtin:true,hidden:false,show_card:true},
    {id:'ask',label:'问问DeepSeek',instruction:'',is_builtin:true,hidden:false,show_card:true},
  ];
  const p = page(c => c === 'config_get' ? Promise.resolve({
    panel_url:'https://work.example/chat',custom_sites:[{label:'工作站点',url:'https://work.example/chat'}],
    hide_on_fullscreen:true,privileged_capture:false,clipboard_fallback_enabled:true,autostart:false,hotkey:'Ctrl+Alt+1',
  }) : c === 'hotkey_status' ? Promise.resolve('Ctrl+Alt+1') : undefined);
  try {
    p.w.__dsfSettingsDataRestored({prompts:restoredPrompts});
    await tick(); await tick();
    assert.match(p.d.getElementById('list').textContent, /迁移后的解释/);
    assert.equal(p.d.getElementById('panel-url-input').value, 'https://work.example/chat');
    assert.equal(p.d.getElementById('hotkey-input').value, 'Ctrl+Alt+1');
    assert.equal(p.d.querySelector('#features input[data-key="fullscreen"]').checked, true);
    assert.ok(p.calls.some(c => c.command === 'config_get'));
    assert.equal(p.d.querySelector('[data-key="clipboard-fallback"]'), null);
    assert.equal(p.d.querySelector('[data-key="privileged"]').checked, false);
    assert.equal(p.calls.filter(c => c.command === 'config_set_clipboard_fallback').length, 0);
    assert.ok(p.calls.some(c => c.command === 'hotkey_status'));
  } finally { p.close(); }
});
