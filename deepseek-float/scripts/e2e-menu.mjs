import assert from 'node:assert/strict';
async function until(predicate, message, timeout = 8000) {
 const deadline = Date.now() + timeout;
 while (Date.now() < deadline) { if (await predicate()) return; await new Promise(resolve => setTimeout(resolve, 100)); }
 throw new Error(message);
}
// 对已用 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9444 启动的本项目实例运行。
// 仅测试本地窗口开关与截图取消，不发送消息、不保存设置。
let targets=[];
for(let attempt=0;attempt<25;attempt++) {
 try {targets=await (await fetch('http://localhost:9444/json/list')).json()}catch{}
 if(['/poc/ball.html','/poc/menu.html','/poc/shot.html','/poc/settings.html'].every(suffix=>targets.some(p=>p.url.endsWith(suffix))))break;
 await new Promise(r=>setTimeout(r,200));
}
async function connect(suffix) {
 const target=targets.find(p=>p.url.endsWith(suffix));assert(target,`missing ${suffix}`);
 const ws=new WebSocket(target.webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r));
 let id=0;const pending=new Map();ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){pending.get(m.id)?.(m);pending.delete(m.id)}});
 const send=(method,params={})=>Promise.race([new Promise(r=>{const n=++id;pending.set(n,r);ws.send(JSON.stringify({id:n,method,params}))}),new Promise((_,reject)=>setTimeout(()=>reject(Error(`timeout ${method}`)),4000))]);
 const evaluate=async expression=>{const m=await send('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!m.result?.exceptionDetails,JSON.stringify(m));return m.result?.result?.value};
 return {send,evaluate,close:()=>ws.close()};
}
const ball=await connect('/poc/ball.html');const menu=await connect('/poc/menu.html');const shot=await connect('/poc/shot.html');const settings=await connect('/poc/settings.html');
try {
 await ball.send('Page.reload');await menu.send('Page.reload');await new Promise(r=>setTimeout(r,900));
 assert.equal(await ball.evaluate(`document.getElementById('ball')?.id`),'ball');
 console.log('PASS ball page loaded');
 if(await ball.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'panel'})`))await ball.evaluate(`__TAURI_INTERNALS__.invoke('toggle_panel')`);
 assert.equal(await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32}).then(()=>true)`),true);
 await new Promise(r=>setTimeout(r,400));
 assert.equal(await menu.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'context-menu'})`),true);
 console.log('PASS right-click IPC resolves and native menu is visible');
 await menu.evaluate(`document.getElementById('panel').click()`);await new Promise(r=>setTimeout(r,300));
 assert.equal(await ball.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'panel'})`),true);
 console.log('PASS menu action opens native panel');
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('toggle_panel')`);
 await ball.send('Input.dispatchMouseEvent',{type:'mousePressed',x:32,y:32,button:'left',clickCount:1});
 await ball.send('Input.dispatchMouseEvent',{type:'mouseReleased',x:32,y:32,button:'left',clickCount:1});
 await new Promise(r=>setTimeout(r,350));
 assert.equal(await ball.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'panel'})`),true);
 console.log('PASS left click still opens panel after menu creation');
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('toggle_panel')`);
 await ball.send('Input.dispatchMouseEvent',{type:'mousePressed',x:32,y:32,button:'right',clickCount:1});
 await ball.send('Input.dispatchMouseEvent',{type:'mouseReleased',x:32,y:32,button:'right',clickCount:1});
 await new Promise(r=>setTimeout(r,300));
 assert.equal(await menu.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'context-menu'})`),true);
 console.log('PASS real browser right click reopens menu');
 await menu.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
 await new Promise(r=>setTimeout(r,250));
 assert.equal(await menu.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'context-menu'})`),false);
 console.log('PASS Escape closes menu and IPC remains responsive');
 for (const action of ['screenshot','ocr']) {
  await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32})`);
  await new Promise(r=>setTimeout(r,150));
  await menu.evaluate(`document.getElementById('${action}').click()`);
  await new Promise(r=>setTimeout(r,250));
  assert.equal(await shot.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'shot'})`),true);
  await shot.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
  await until(async () =>
    !await shot.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'shot'})`) &&
    await ball.evaluate(`getComputedStyle(document.documentElement).visibility`) === 'visible',
    `${action}: overlay cleanup must restore the ball`);
  assert.equal(await shot.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'shot'})`),false);
  assert.equal(await ball.evaluate(`getComputedStyle(document.documentElement).visibility`),'visible');
  console.log(`PASS ${action} overlay opens, cancels and restores ball`);
 }
 await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32})`);
 await new Promise(r=>setTimeout(r,150));
 await menu.evaluate(`document.getElementById('settings').click()`);
 await new Promise(r=>setTimeout(r,300));
 assert.equal(await settings.evaluate(`__TAURI_INTERNALS__.invoke('plugin:window|is_visible',{label:'settings'})`),true);
 assert.equal(await settings.evaluate(`__TAURI_INTERNALS__.invoke('config_get').then(c=>!!c&&typeof c==='object')`),true);
 console.log('PASS settings window opens and config read succeeds without changing settings');

 await ball.evaluate(`__TAURI_INTERNALS__.invoke('ball_contextmenu',{x:32,y:32})`);
 await new Promise(r=>setTimeout(r,150));
 const layout=await menu.evaluate(`(()=>{const box=document.querySelector('main').getBoundingClientRect();return {width:innerWidth,height:innerHeight,bottom:box.bottom,rows:[...document.querySelectorAll('button')].map(b=>({id:b.id,top:b.getBoundingClientRect().top,bottom:b.getBoundingClientRect().bottom}))}})()`);
 assert.equal(layout.width,200);assert.equal(layout.height,180);assert(layout.bottom<=180);assert(layout.rows.every(r=>r.top>=0&&r.bottom<=180));
 console.log('PASS compact 200x180 native viewport fits all eight main actions');
 await menu.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27});
} finally {ball.close();menu.close();shot.close();settings.close()}
