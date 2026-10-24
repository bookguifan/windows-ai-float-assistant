import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

function page() {
  const html = readFileSync(new URL('../poc/menu.html', import.meta.url), 'utf8');
  const dom = new JSDOM(html, { runScripts: 'outside-only' });
  const calls = [];
  dom.window.__TAURI_INTERNALS__ = { invoke: (command, args) => {
    calls.push({ command, args }); return Promise.resolve();
  } };
  for (const script of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)) dom.window.eval(script[1]);
  return { window: dom.window, document: dom.window.document, calls, close: () => dom.window.close() };
}

function init(p, options = {}) {
  p.window.__dsfMenuSet({ generation: 7, ballVisible: true, panelVisible: false,
    hotkey: 'Ctrl+Alt+1', shotBusy: false, ...options });
}

test('shared menu retains the tray login refresh action', () => {
  const p = page();
  try {
    init(p);
    p.document.getElementById('relogin').click();
    assert.equal(p.calls.at(-1).args.action, 'relogin');
  } finally { p.close(); }
});

test('menu shows one state-aware action for each window and the actual hotkey', () => {
  const p = page();
  try {
    init(p);
    assert.equal(p.document.getElementById('ballLabel').textContent, '隐藏悬浮球');
    assert.equal(p.document.getElementById('panelLabel').textContent, '打开对话面板');
    assert.equal(p.document.getElementById('hotkey').textContent, 'Ctrl+Alt+1');
    init(p, { generation: 8, ballVisible: false, panelVisible: true, hotkey: null });
    assert.equal(p.document.getElementById('ballLabel').textContent, '显示悬浮球');
    assert.equal(p.document.getElementById('panelLabel').textContent, '收起对话面板');
    assert.equal(p.document.getElementById('hotkey').textContent, '');
  } finally { p.close(); }
});

test('an action is sent once with its generation and explicit target state', () => {
  const p = page();
  try {
    init(p);
    const row = p.document.getElementById('panel');
    row.click(); row.click();
    const actions = p.calls.filter(c => c.command === 'context_menu_action');
    assert.equal(actions.length, 1);
    assert.equal(actions[0].args.action, 'show_panel');
    assert.equal(actions[0].args.generation, 7);
  } finally { p.close(); }
});

test('busy screenshot actions are disabled but settings remains available', () => {
  const p = page();
  try {
    init(p, { shotBusy: true });
    p.document.getElementById('screenshot').click();
    assert.equal(p.calls.filter(c => c.command === 'context_menu_action').length, 0);
    p.document.getElementById('settings').click();
    assert.equal(p.calls.at(-1).args.action, 'settings');
  } finally { p.close(); }
});

test('keyboard navigation skips disabled actions and Escape closes this generation', () => {
  const p = page();
  try {
    init(p, { shotBusy: true });
    p.document.getElementById('panel').focus();
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown', { key: 'ArrowDown' }));
    assert.equal(p.document.activeElement.id, 'switchSite');
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown', { key: 'ArrowDown' }));
    assert.equal(p.document.activeElement.id, 'favorites');
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown', { key: 'ArrowDown' }));
    assert.equal(p.document.activeElement.id, 'settings');
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown', { key: 'Escape' }));
    assert.equal(p.calls.at(-1).command, 'context_menu_close');
    assert.equal(p.calls.at(-1).args.generation, 7);
  } finally { p.close(); }
});

test('late menu data cannot replace the newer generation', () => {
  const p = page();
  try {
    init(p, { generation: 8, ballVisible: false });
    init(p, { generation: 7, ballVisible: true });
    p.document.getElementById('ball').click();
    assert.equal(p.calls.at(-1).args.action, 'show_ball');
    assert.equal(p.calls.at(-1).args.generation, 8);
  } finally { p.close(); }
});

test('invalid menu payload does not activate actions or close commands', () => {
 const p=page();try {for(const generation of [0,-1,NaN,Infinity,9007199254740992]) assert.equal(p.window.__dsfMenuSet({generation}),false);p.document.getElementById('quit').click();assert.equal(p.calls.filter(c=>c.command==='context_menu_action').length,0)}finally{p.close()}
});
test('background dismiss is consumed once and cannot be followed by an action', () => {
 const p=page();try{init(p);p.document.body.dispatchEvent(new p.window.MouseEvent('mousedown',{bubbles:true}));p.document.getElementById('quit').click();assert.equal(p.calls.filter(c=>c.command==='context_menu_close').length,1);assert.equal(p.calls.filter(c=>c.command==='context_menu_action').length,0)}finally{p.close()}
});
test('Home End and reverse Tab wrap over enabled menu actions', () => {
 const p=page();try{init(p);for(const [key,shiftKey,want] of [['End',false,'quit'],['Home',false,'ball'],['Tab',true,'quit']]){p.window.dispatchEvent(new p.window.KeyboardEvent('keydown',{key,shiftKey}));assert.equal(p.document.activeElement.id,want)}}finally{p.close()}
});

test('site submenu marks current site and Escape returns without closing',()=>{const p=page();try{init(p,{panelUrl:'https://chatgpt.com/',sites:[{label:'DeepSeek',url:'https://chat.deepseek.com/'},{label:'ChatGPT',url:'https://chatgpt.com/'}]});p.document.getElementById('switchSite').click();assert.equal(p.document.getElementById('siteView').hidden,false);assert.equal(p.document.querySelector('[data-site][aria-checked="true"]').dataset.site,'https://chatgpt.com/');p.window.dispatchEvent(new p.window.KeyboardEvent('keydown',{key:'Escape'}));assert.equal(p.document.getElementById('siteView').hidden,true);assert.equal(p.calls.filter(c=>c.command==='context_menu_close').length,0)}finally{p.close()}});
test('selecting site sends URL and current generation exactly once',()=>{const p=page();try{init(p,{sites:[{label:'DeepSeek',url:'https://chat.deepseek.com/'}]});p.document.getElementById('switchSite').click();const item=p.document.querySelector('[data-site]');item.click();item.click();const calls=p.calls.filter(c=>c.command==='context_menu_switch_site');assert.equal(calls.length,1);assert.equal(calls[0].args.generation,7);assert.equal(calls[0].args.url,'https://chat.deepseek.com/')}finally{p.close()}});

test('site failure restores retry and retains the active menu generation',async()=>{const p=page();try{init(p,{sites:[{label:'Site',url:'https://example.test/'}]});const calls=[];p.window.__TAURI_INTERNALS__.invoke=(command,args)=>{calls.push({command,args});return Promise.reject(new Error('切换失败'))};p.document.getElementById('switchSite').click();const item=p.document.querySelector('[data-site]');item.click();await Promise.resolve();assert.match(p.document.getElementById('siteError').textContent,/切换失败/);item.click();await Promise.resolve();assert.equal(calls.length,2);assert.equal(calls[1].args.generation,7)}finally{p.close()}});
test('menu has unique DOM ids and nine main actions',()=>{const p=page();try{const ids=[...p.document.querySelectorAll('[id]')].map(e=>e.id);assert.equal(ids.length,new Set(ids).size);assert.equal(p.document.querySelectorAll('#mainView button').length,9)}finally{p.close()}});

test('favorites shortcut sends one action for the current menu generation', () => {
  const p = page();
  try {
    init(p);
    const button = p.document.getElementById('favorites');
    assert.ok(button);
    button.click(); button.click();
    assert.deepEqual(p.calls.filter(c => c.command === 'context_menu_action').map(c => ({...c.args})),
      [{generation: 7, action: 'favorites'}]);
  } finally { p.close(); }
});
