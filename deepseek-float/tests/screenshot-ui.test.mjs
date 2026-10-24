import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const html = readFileSync(new URL('../poc/shot.html', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));

test('selected screenshot is delivered automatically without a preview prompt', async () => {
  const p = page();
  try {
    p.window.__TAURI__.core.invoke = (command,args) => {
      p.calls.push({command,args});
      return Promise.resolve(command === 'shot_commit' ? {captureId:args.captureId,revision:args.revision} : true);
    };
    p.window.__dsfShotReset(71,1); drag(p.window); await tick(); await tick();
    assert.deepEqual(p.calls.map(c=>[c.command,c.args.captureId,c.args.revision]), [
      ['shot_commit',71,1],['shot_preview_ready',71,1],['shot_confirm',71,1],
    ]);
    assert.equal(p.window.document.querySelector('#preview').hidden,true);
    assert.equal(p.window.document.querySelector('#confirm'),null);
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,1);
  } finally {p.close();}
});

test('OCR screenshot is delivered automatically through the same no-preview path', async () => {
  const p = page();
  try {
    p.window.__TAURI__.core.invoke = (command,args) => {
      p.calls.push({command,args});
      return Promise.resolve(command === 'shot_commit' ? {captureId:args.captureId,revision:args.revision} : true);
    };
    p.window.__dsfShotReset(72,1); drag(p.window); await tick(); await tick();
    assert.deepEqual(p.calls.map(c=>[c.command,c.args.captureId,c.args.revision]), [
      ['shot_commit',72,1],['shot_preview_ready',72,1],['shot_confirm',72,1],
    ]);
    assert.equal(p.calls.at(-1).args.captureId,72);
    assert.equal(p.window.document.querySelector('#preview').hidden,true);
    assert.equal(p.window.document.querySelector('#confirm'),null);
  } finally {p.close();}
});

test('late capture response cannot confirm a cancelled or newer capture', async () => {
  const p=page(); let resolve;
  try {
    p.window.__TAURI__.core.invoke=(command,args)=>{p.calls.push({command,args});return command==='shot_commit'?new Promise(r=>resolve=r):Promise.resolve(true)};
    p.window.__dsfShotReset(81,1); drag(p.window);
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown',{key:'Escape'}));
    resolve({captureId:81,revision:1}); await tick();
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,0);
    p.window.__dsfShotReset(82,1); drag(p.window);
    p.window.__dsfShotReset(83,1);
    resolve({captureId:82,revision:1}); await tick();
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,0);
    assert.equal(p.calls.filter(c=>c.command==='shot_preview_ready').length,0);
  } finally {p.close();}
});

test('failed capture can be reselected with the same ID and a newer revision', async () => {
  const p=page();
  try {
    p.window.__TAURI__.core.invoke=(command,args)=>{p.calls.push({command,args});return command==='shot_commit'?Promise.reject('fixture error'):Promise.resolve(command==='shot_reselect'?2:true)};
    p.window.__dsfShotReset(91,1); drag(p.window); await tick();
    assert.match(p.window.document.querySelector('#preview-status').textContent,/fixture error/);
    assert.equal(p.window.document.querySelector('#confirm'),null);
    p.window.document.querySelector('#reselect').click(); await tick();
    p.window.__dsfShotReset(91,1);
    drag(p.window);
    assert.equal(p.calls.at(-1).args.captureId,91);
    assert.equal(p.calls.at(-1).args.revision,2);
    await tick();
    p.window.document.querySelector('#cancel').click();
    assert.equal(p.calls.at(-1).command,'shot_cancel');
    assert.equal(p.calls.at(-1).args.revision,2);
  } finally {p.close();}
});

test('workflow invalidation prevents a late capture response from auto-confirming', async () => {
  const p=page();
  let resolve;
  try {
    p.window.__TAURI__.core.invoke=(command,args)=>{p.calls.push({command,args});return command==='shot_commit'?new Promise(r=>resolve=r):Promise.resolve(true)};
    p.window.__dsfShotReset(101,1); drag(p.window);
    assert.equal(typeof p.window.__dsfShotInvalidate,'function');
    p.window.__dsfShotInvalidate(101);
    resolve({captureId:101,revision:1}); await tick();
    assert.equal(p.window.document.querySelector('#preview').hidden,true);
    assert.equal(p.calls.filter(c=>c.command==='shot_preview_ready').length,0);
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,0);
  } finally {p.close();}
});

test('same-ID reselection rejects stale callbacks from a previous revision', async () => {
  const p=page();
  try {
    p.window.__TAURI__.core.invoke=(command,args)=>{p.calls.push({command,args});return command==='shot_commit'?Promise.reject('capture error'):Promise.resolve(command==='shot_reselect'?2:true)};
    p.window.__dsfShotReset(111,1); drag(p.window); await tick();
    p.window.document.querySelector('#reselect').click(); await tick();
    const before=p.calls.length;
    assert.equal(p.window.__dsfShotReselect(111,1),false);
    assert.equal(p.calls.length,before);
    drag(p.window); assert.equal(p.calls.at(-1).args.revision,2);
  } finally {p.close();}
});

test('delivery failure offers reselect and never confirms the consumed draft again', async () => {
  const p=page();
  try {
    p.window.__TAURI__.core.invoke=(command,args)=>{p.calls.push({command,args});return Promise.resolve(command==='shot_commit'?{captureId:args.captureId,revision:args.revision}:command==='shot_reselect'?2:true)};
    p.window.__dsfShotReset(121,1); drag(p.window); await tick(); await tick();
    p.window.__dsfShotDeliveryError(121,1,'fixture delivery error'); await tick();
    assert.match(p.window.document.querySelector('#preview-status').textContent,/fixture delivery error/);
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,1);
    p.window.document.querySelector('#reselect').click(); await tick(); drag(p.window); await tick(); await tick();
    assert.equal(p.calls.at(-1).args.revision,2);
    assert.equal(p.calls.filter(c=>c.command==='shot_confirm').length,2);
  } finally {p.close();}
});

function page() {
  const dom = new JSDOM(html, { runScripts: 'outside-only' });
  const calls = [];
  dom.window.__TAURI__ = {
    core: { invoke: (command, args) => { calls.push({ command, args }); return Promise.resolve(); } },
  };
  for (const match of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)) {
    dom.window.eval(match[1]);
  }
  return { window: dom.window, calls, close: () => dom.window.close() };
}

function drag(window) {
  for (const [type, clientX, clientY] of [['mousedown', 10, 20], ['mouseup', 110, 100]]) {
    window.dispatchEvent(new window.MouseEvent(type, { button: 0, clientX, clientY }));
  }
}

test('an uninitialized overlay never submits a capture', () => {
  const p = page();
  try { drag(p.window); assert.equal(p.calls.length, 0); } finally { p.close(); }
});

test('selection commits once with the capture ID assigned by Rust', () => {
  const p = page();
  try {
    p.window.__dsfShotReset(41);
    drag(p.window);
    drag(p.window);
    assert.equal(p.calls.length, 1);
    assert.equal(p.calls[0].command, 'shot_commit');
    assert.equal(p.calls[0].args.captureId, 41);
    assert.deepEqual(JSON.parse(JSON.stringify(p.calls[0].args.rect)), {
      x: 10, y: 20, w: 100, h: 80, dpr: 1, vw: 1024, vh: 768,
    });
  } finally { p.close(); }
});

test('cancel carries its capture ID and a later reset starts a distinct selection', () => {
  const p = page();
  try {
    p.window.__dsfShotReset(41);
    p.window.dispatchEvent(new p.window.KeyboardEvent('keydown', { key: 'Escape' }));
    drag(p.window);
    assert.equal(p.calls.length, 1);
    assert.equal(p.calls[0].command, 'shot_cancel');
    assert.equal(p.calls[0].args.captureId, 41);
    p.window.__dsfShotReset(42);
    drag(p.window);
    assert.equal(p.calls.length, 2);
    assert.equal(p.calls[1].args.captureId, 42);
  } finally { p.close(); }
});

test('a delayed reset from an older capture does not overwrite the current ID', () => {
  const p = page();
  try {
    p.window.__dsfShotReset(42);
    p.window.__dsfShotReset(41);
    drag(p.window);
    assert.equal(p.calls[0].args.captureId, 42);
  } finally { p.close(); }
});

test('readiness retry for the same capture preserves an in-progress drag', () => {
  const p = page();
  try {
    p.window.__dsfShotReset(41);
    p.window.dispatchEvent(new p.window.MouseEvent('mousedown', { button: 0, clientX: 10, clientY: 20 }));
    p.window.__dsfShotReset(41);
    p.window.dispatchEvent(new p.window.MouseEvent('mouseup', { button: 0, clientX: 110, clientY: 100 }));
    assert.equal(p.calls.length, 1);
    assert.equal(p.calls[0].args.rect.w, 100);
  } finally { p.close(); }
});

test('right click cancels the active capture only once',()=>{const p=page();try{p.window.__dsfShotReset(51);p.window.dispatchEvent(new p.window.MouseEvent('contextmenu',{cancelable:true}));drag(p.window);assert.equal(p.calls.length,1);assert.equal(p.calls[0].command,'shot_cancel');assert.equal(p.calls[0].args.captureId,51)}finally{p.close()}});
test('tiny selection cancels instead of submitting',()=>{const p=page();try{p.window.__dsfShotReset(51);for(const [type,x,y] of [['mousedown',10,10],['mouseup',12,12]])p.window.dispatchEvent(new p.window.MouseEvent(type,{button:0,clientX:x,clientY:y}));assert.equal(p.calls[0].command,'shot_cancel')}finally{p.close()}});
test('reverse direction selection produces normalized positive rectangle',()=>{const p=page();try{p.window.__dsfShotReset(51);for(const [type,x,y] of [['mousedown',110,100],['mouseup',10,20]])p.window.dispatchEvent(new p.window.MouseEvent(type,{button:0,clientX:x,clientY:y}));assert.equal(p.calls[0].command,'shot_commit');assert.equal(p.calls[0].args.rect.x,10);assert.equal(p.calls[0].args.rect.y,20);assert.equal(p.calls[0].args.rect.w,100);assert.equal(p.calls[0].args.rect.h,80)}finally{p.close()}});
