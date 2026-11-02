import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM, VirtualConsole } from 'jsdom';
function setup({ diagnostic = false, installPageDiagnosticBridge = false, invoke, bridge = true } = {}) {
 const html=readFileSync(new URL('../poc/ball.html',import.meta.url),'utf8');
 const dom=new JSDOM(html,{runScripts:'outside-only',pretendToBeVisual:true,virtualConsole:new VirtualConsole()});
 const diagnosticListeners=[];
 const addEventListener=dom.window.addEventListener.bind(dom.window);
 dom.window.addEventListener=(type,...args)=>{
  if(type==='error'||type==='unhandledrejection')diagnosticListeners.push(type);
  return addEventListener(type,...args);
 };
 const calls=[]; const stages=[];
 if (diagnostic) dom.window.__DSFLOAT_BALL_DIAG__=stage=>stages.push(stage);
 if (bridge) dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{calls.push({command,args});return invoke ? invoke(command,args) : Promise.resolve()}};
 if (installPageDiagnosticBridge) dom.window.eval(readFileSync(new URL('../src-tauri/src/ball_page_diag.js',import.meta.url),'utf8'));
 for(const script of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi))dom.window.eval(script[1]);
 const ball=dom.window.document.getElementById('ball');
 const mouse=(type,x=32,y=32,button=0)=>ball.dispatchEvent(new dom.window.MouseEvent(type,{bubbles:true,cancelable:true,clientX:x,clientY:y,button}));
 return {dom,calls,stages,diagnosticListeners,mouse,count:command=>calls.filter(c=>c.command===command).length};
}

test('global page error monitoring is registered only once by the diagnostic bridge',()=>{
 const ordinary=setup();
 const diagnostic=setup({installPageDiagnosticBridge:true});
 try {
  assert.deepEqual(ordinary.diagnosticListeners,[]);
  assert.deepEqual(diagnostic.diagnosticListeners,['error','unhandledrejection']);
 } finally {ordinary.dom.window.close();diagnostic.dom.window.close()}
});

test('diagnostic handshake separates JS, IPC and successful command completion',async()=>{
 const p=setup({diagnostic:true});
 try {
  p.dom.window.notifyBallReady();
  await Promise.resolve();
  assert.deepEqual(p.stages,['script-start','ipc-available','ready-invoke-start','ready-invoke-ok']);
  p.dom.window.notifyBallReady();
  assert.equal(p.count('ball_ready'),1);
 } finally {p.dom.window.close()}
});
test('diagnostic rejected IPC never reports Ready or leaks its error text',async()=>{
 const p=setup({diagnostic:true,invoke:()=>Promise.reject(new Error('private-test-error'))});
 try {
  p.dom.window.notifyBallReady();
  await Promise.resolve();await Promise.resolve();
  assert.ok(p.stages.includes('ready-invoke-rejected'));
  assert.ok(!p.stages.includes('ready-invoke-ok'));
  assert.ok(!p.stages.join(' ').includes('private-test-error'));
 } finally {p.dom.window.close()}
});
test('diagnostic missing bridge is distinct from a loaded page',async()=>{
 const p=setup({diagnostic:true,bridge:false});
 try {
  p.dom.window.notifyBallReady();
  await Promise.resolve();await Promise.resolve();
  assert.ok(p.stages.includes('ipc-unavailable'));
  assert.ok(p.stages.includes('ready-invoke-rejected'));
  assert.ok(!p.stages.includes('ready-invoke-ok'));
 } finally {p.dom.window.close()}
});
test('ordinary page sends only the original Ready command without diagnostic IPC',async()=>{
 const p=setup();
 try {p.dom.window.notifyBallReady();await Promise.resolve();assert.deepEqual(p.calls.map(x=>x.command),['ball_ready'])}
 finally {p.dom.window.close()}
});
test('left click opens panel once and rapid double click is debounced',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mouseup');p.mouse('mousedown');p.mouse('mouseup');assert.equal(p.count('toggle_panel'),1)}finally{p.dom.window.close()}});
test('drag beyond threshold starts native drag once and never toggles panel',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mousemove',40,40);p.mouse('mousemove',50,50);p.mouse('mouseup',50,50);assert.equal(p.count('plugin:window|start_dragging'),1);assert.equal(p.count('toggle_panel'),0)}finally{p.dom.window.close()}});
test('small movement remains a click',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mousemove',34,34);p.mouse('mouseup',34,34);assert.equal(p.count('toggle_panel'),1);assert.equal(p.count('plugin:window|start_dragging'),0)}finally{p.dom.window.close()}});
test('blur cancels unfinished click',()=>{const p=setup();try{p.mouse('mousedown');p.dom.window.dispatchEvent(new p.dom.window.Event('blur'));p.mouse('mouseup');assert.equal(p.count('toggle_panel'),0)}finally{p.dom.window.close()}});
test('right click only requests the context menu',()=>{const p=setup();try{p.mouse('mousedown',32,32,2);p.mouse('mouseup',32,32,2);p.mouse('contextmenu',32,32,2);assert.equal(p.count('ball_contextmenu'),1);assert.equal(p.count('toggle_panel'),0);assert.equal(p.count('plugin:window|start_dragging'),0)}finally{p.dom.window.close()}});

test('injected diagnostic bridge forwards lifecycle and Ready stages without leaking arbitrary payloads',async()=>{
 const dom=new JSDOM('<!doctype html><body></body>',{runScripts:'outside-only'});
 const calls=[];
 dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{calls.push({command,args});return Promise.resolve()}};
 try {
  dom.window.eval(readFileSync(new URL('../src-tauri/src/ball_page_diag.js',import.meta.url),'utf8'));
  const expected=['script-start','ipc-available','ipc-unavailable','first-frame-callback','ready-invoke-start','ready-invoke-ok','ready-invoke-rejected'];
  for(const stage of expected) dom.window.__DSFLOAT_BALL_DIAG__(stage);
  dom.window.__DSFLOAT_BALL_DIAG__('private-test-payload');
  assert.deepEqual(calls.map(x=>x.args.stage),['document-start',...expected]);
  assert.ok(calls.every(x=>x.command==='ball_diagnostic'));
  dom.window.document.dispatchEvent(new dom.window.Event('DOMContentLoaded'));
  assert.ok(calls.some(x=>x.args.stage==='dom-content-loaded'));
  assert.ok(!calls.some(x=>JSON.stringify(x).includes('private-test-payload')));
 } finally {dom.window.close()}
});
