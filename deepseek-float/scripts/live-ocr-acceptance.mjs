// Opt-in real image transcription benchmark. This does not simulate AI answers.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {connect} from './lib/live-cdp.mjs';
import {scoreTranscript} from './lib/ocr-score.mjs';
const root=fileURLToPath(new URL('..',import.meta.url));
const args=process.argv.slice(2);
const option=(key,fallback)=>{const i=args.indexOf(key);return i<0?fallback:args[i+1];};
if(args.includes('--help')){
 console.log('node scripts/live-ocr-acceptance.mjs --allow-live --site deepseek --local-dir .test-artifacts/live-20261017/local [--benchmark .test-artifacts/ocr-benchmark] [--port 9444]');
 process.exit(0);
}
assert(args.includes('--allow-live'),'Requires --allow-live: uploads four synthetic benchmark images to the selected AI site.');
const sites={deepseek:'https://chat.deepseek.com/',chatgpt:'https://chatgpt.com/',gemini:'https://gemini.google.com/app',kimi:'https://www.kimi.com/',chatglm:'https://chatglm.cn/main/alltoolsdetail?lang=zh',qianwen:'https://www.qianwen.com/'};
const site=option('--site','deepseek');
assert(sites[site]);
assert(option('--local-dir'),'Specify an isolated --local-dir.');
const local=fs.realpathSync(path.resolve(root,option('--local-dir')));
assert(local.startsWith(fs.realpathSync(path.join(root,'.test-artifacts'))+path.sep),'Isolation must be inside .test-artifacts.');
const config=path.join(local,'com.deepseekfloat.desktop/config.json');
const benchmark=path.resolve(root,option('--benchmark','.test-artifacts/ocr-benchmark'));
const manifest=JSON.parse(fs.readFileSync(path.join(benchmark,'manifest.json'),'utf8'));
const port=Number(option('--port','9444'));
assert(Number.isInteger(port)&&port>0&&port<65536);
const prompt=fs.readFileSync(path.join(root,'src-tauri/src/screenshot.rs'),'utf8').match(/const OCR_PROMPT: &str = "([^"]+)";/)?.[1];
assert(prompt,'Use the exact application OCR instruction.');
const report={site,started:new Date().toISOString(),scope:'Real PNG upload through application attachment API, fixed OCR prompt, native result readback; native capture/preview are a separate workflow test.',cases:[]};
const output=path.join(benchmark,`${site}-${Date.now().toString(36)}.json`);
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const clients=[];
let invoke,original,panel;
try{
 const ball=await connect(port,p=>p.url.endsWith('/poc/ball.html'));clients.push(ball);
 panel=await connect(port,p=>p.url.startsWith('https://'));clients.push(panel);
 const card=await connect(port,p=>p.url.endsWith('/poc/result.html'));clients.push(card);
 invoke=(c,a={})=>ball.evaluate(`__TAURI_INTERNALS__.invoke(${JSON.stringify(c)},${JSON.stringify(a)})`);
 original=(await invoke('config_get')).panel_url||sites.deepseek;
 const modified=fs.statSync(config).mtimeMs;
 const proofUrl=original===sites[site]?sites[site]+'#dsf-isolation-'+Date.now().toString(36):sites[site];
 await invoke('config_set_panel_url',{url:proofUrl});
 assert(fs.statSync(config).mtimeMs>modified,'Running instance does not write the isolated config.');
 if(proofUrl!==sites[site])await invoke('config_set_panel_url',{url:sites[site]});
 async function ready(){
  for(let i=0;i<90;i++){
   await sleep(500);
   try {if(await panel.evaluate(`location.hostname===${JSON.stringify(new URL(sites[site]).hostname)}&&document.readyState==='complete'&&!!window.__DSFLOAT__&&!window.__dsfloatNavigationPending&&!!window.__DSFLOAT__.inspect().hasInput`))return;}catch{}
  }
  throw Error('Site not ready or not logged in. No image uploaded.');
 }
 await ready();
 for(const item of manifest.cases){
  const result={id:item.id,kind:item.kind,expected:item.expected,sha256:item.sha256};
  report.cases.push(result);
  await invoke('result_new_session');await ready();await sleep(1500);
  // Composer text can hydrate before its upload inputs. Observe availability before
  // the single attachment attempt; never retry a transfer whose outcome is unknown.
  for(let i=0;i<60;i++){
   result.uploadControlReady=await panel.evaluate(`(()=>{const input=document.querySelector('textarea,[contenteditable="true"]');const root=location.hostname==='chatgpt.com'?input?.closest('[data-composer-surface-variant]'):document;return !!root?.querySelector('input[type="file"]:not([capture]):not([disabled])')})()`);
   if(result.uploadControlReady)break;
   await sleep(500);
  }
  assert(result.uploadControlReady,'Upload control did not hydrate; no attachment attempted.');
  const bytes=fs.readFileSync(path.join(benchmark,item.file));
  assert.equal(createHash('sha256').update(bytes).digest('hex'),item.sha256,'Benchmark image checksum mismatch.');
  assert.equal(await panel.evaluate('__DSFLOAT__.shotReset()'),true);
  const encoded=bytes.toString('base64');
  let seq=0;
  for(let start=0;start<encoded.length;start+=262144){
   assert.equal(await panel.evaluate(`__DSFLOAT__.shotPush(${JSON.stringify(encoded.slice(start,start+262144))},${seq++})`),true);
  }
  assert.equal(await panel.evaluate(`__DSFLOAT__.shotCommit('image/png')`),true,'No scoped image input accepted the file.');
  result.uploadCalls=1;
  for(let i=0;i<100;i++){
   await sleep(300);
   result.attachmentConfirmed=await panel.evaluate('__DSFLOAT__.hasAttachment()');
   if(result.attachmentConfirmed)break;
  }
  assert(result.attachmentConfirmed,'No new attachment confirmed; OCR instruction was not sent.');
  await sleep(1000);
  await invoke('result_ask',{text:prompt}); // One submission, never automatically retried.
  result.submitCalls=1;
  const started=Date.now();
  for(let i=0;i<180;i++){
   await sleep(500);
   const s=await card.evaluate(`(()=>{const t=[...document.querySelectorAll('#turns .turn')].at(-1);return {question:t?.querySelector('.ask')?.textContent,text:t?.querySelector('.md')?.innerText,html:t?.querySelector('.md')?.innerHTML,notes:[...t?.querySelectorAll('.note')||[]].map(n=>n.textContent),streaming:!!t?.querySelector('.caret')}})()`);
   if(s.question!==prompt)continue;
   if(s.notes.includes('已完成')&&!s.streaming){result.actual=s.text;result.html=s.html;result.elapsedMs=Date.now()-started;break;}
   if(s.notes.some(n=>/超时|不确定|失败|已取消/.test(n)))throw Error(JSON.stringify(s));
  }
  assert(result.actual,'No completed OCR result.');
  result.score=scoreTranscript(item.expected,result.actual,item.kind);
  result.outcome=result.score.exact && result.score.linesPreserved!==false?'pass':'quality-mismatch';
  fs.writeFileSync(output,JSON.stringify(report,null,2));
  console.log(JSON.stringify({case:item.id,outcome:result.outcome,score:result.score}));
 }
 report.outcome=report.cases.every(c=>c.outcome==='pass')?'pass':'quality-mismatch';
 if(report.outcome!=='pass')process.exitCode=1;
}catch(e){report.outcome='workflow-failed';report.error=String(e.stack||e);process.exitCode=1;
 if(panel)try{report.failureState=await panel.evaluate(`({url:location.origin+location.pathname,inspect:window.__DSFLOAT__?.inspect(),fileInputs:[...document.querySelectorAll('input[type=file]')].map(n=>({accept:n.accept,capture:n.hasAttribute('capture'),disabled:n.disabled})),loading:document.querySelectorAll('.ds-loading,[aria-busy="true"],[role=progressbar]').length,images:[...document.images].map(n=>({alt:n.alt,complete:n.complete,naturalWidth:n.naturalWidth}))})`);}catch{}
}
finally{
 if(invoke&&original)try{await invoke('config_set_panel_url',{url:original});report.restored=original;}catch(e){report.restoreError=String(e);process.exitCode=1;}
 clients.forEach(c=>c.close());report.finished=new Date().toISOString();
 fs.writeFileSync(output,JSON.stringify(report,null,2));
 console.log(JSON.stringify({site,outcome:report.outcome,error:report.error,report:output}));
}
