// Minimal localhost-only CDP client for opt-in native WebView acceptance.
export async function connect(port, match) {
  let page;
  for(let attempt=0;attempt<20;attempt++) {
    try {
      const pages=await (await fetch(`http://127.0.0.1:${port}/json/list`,{signal:AbortSignal.timeout(2000)})).json();
      page=pages.find(match);
      if(page)break;
    } catch { /* Startup may not have opened its localhost endpoint yet. */ }
    await new Promise(resolve=>setTimeout(resolve,500));
  }
  if(!page) throw Error('Required application WebView is absent.');
  const ws=new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error('CDP connection timed out.')),10000);
    ws.addEventListener('open',()=>{clearTimeout(timer);resolve();},{once:true});
    ws.addEventListener('error',e=>{clearTimeout(timer);reject(e);},{once:true});
  });
  let id=0;
  const pending=new Map();
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);pending.get(m.id)?.(m);pending.delete(m.id);});
  return {close:()=>ws.close(),evaluate(expression){return new Promise((resolve,reject)=>{
    const n=++id;
    const timer=setTimeout(()=>{pending.delete(n);reject(Error('CDP evaluation timed out; no submission is retried.'));},15000);
    pending.set(n,m=>{clearTimeout(timer);m.error||m.result.exceptionDetails?reject(Error(JSON.stringify(m.error||m.result.exceptionDetails))):resolve(m.result.result.value);});
    ws.send(JSON.stringify({id:n,method:'Runtime.evaluate',params:{expression,returnByValue:true,awaitPromise:true}}));
  });}};
}
