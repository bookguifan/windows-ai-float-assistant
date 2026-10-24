import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';
function page() {
 const html=readFileSync(new URL('../poc/result.html',import.meta.url),'utf8');
 const dom=new JSDOM(html,{runScripts:'outside-only',url:'https://tauri.localhost'}); const calls=[];
 dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{calls.push({command,args});return Promise.resolve('ok');}};
 dom.window.ResizeObserver=class {observe(){} disconnect(){}};
 for(const s of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)) dom.window.eval(s[1]);
 return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};
}
test('failed send offers one retry with request identity and keeps the question',()=>{
 const p=page();try{p.w.__dsfResultSet({turns:[{quote:'完整问题',sendState:'failed',sendMessage:'未确认发送',request:42}]});
 assert.match(p.d.body.textContent,/未确认发送/); const b=p.d.querySelector('[data-send-retry]');assert.ok(b);b.click();b.click();
 assert.equal(p.calls.filter(c=>c.command==='result_retry_send').length,1); assert.equal(p.calls.at(-1).args.request,42);
 assert.match(p.d.body.textContent,/完整问题/);assert.equal(p.d.querySelector('.resume-hint'),null);
 }finally{p.close();}
});
test('reading and timeout have distinct actions',()=>{
 const p=page();try{p.w.__dsfResultSet({turns:[{quote:'问题',sendState:'reading',streaming:true}]});assert.match(p.d.body.textContent,/读取中/);
 p.w.__dsfResultStream({sendState:'timed-out',note:'回答可能未完成',streaming:false,request:42});
 assert.equal(p.d.querySelector('[data-send-retry]'),null); const b=p.d.querySelector('.resume-hint');b.click();b.click();
 assert.equal(p.calls.filter(c=>c.command==='result_resume').length,1);
 }finally{p.close();}
});
// Exercise the actual Rust-generated JS templates in a document with reordered evaluations.
function workflowScript(name, ...args) {
 const source=readFileSync(new URL('../src-tauri/src/workflow.rs',import.meta.url),'utf8');
 const section=source.slice(source.indexOf('pub fn '+name+'('));
 const template=section.match(/format!\(\s*"([^"\n]+)"/)[1]; let i=0;
 return template.replace(/\{\}/g,()=>String(args[i++])).replaceAll('{{','{').replaceAll('}}','}');
}
test('late initialization never restores an old request and queued scripts cannot execute',()=>{
 const p=page();try{
 p.w.executed=0;p.w.eval(workflowScript('initialize_script',9));p.w.eval(workflowScript('initialize_script',8));
 assert.equal(p.w.__dsfloatRequest,9);
 assert.equal(p.w.eval(workflowScript('guarded',8,"(window.executed++,'ran')")),'no-api');
 assert.equal(p.w.executed,0);
 assert.equal(p.w.eval(workflowScript('guarded',9,"(window.executed++,'ran')")),'ran');assert.equal(p.w.executed,1);
 p.w.__dsfloatNavigationPending=true;
 assert.equal(p.w.eval(workflowScript('guarded',9,"(window.executed++,'ran')")),'no-api');assert.equal(p.w.executed,1);
 delete p.w.__dsfloatRequest;p.w.__dsfloatNavigationPending=false;
 assert.equal(p.w.eval(workflowScript('guarded',9,"(window.executed++,'ran')")),'no-api');assert.equal(p.w.executed,1);
 }finally{p.close();}
});

test('uncertain delivery has no retry action and keeps the original question',()=>{const p=page();try{p.w.__dsfResultSet({turns:[{quote:'保留完整问题',sendState:'uncertain',sendMessage:'发送结果不确定，请打开面板核对；不会自动重复提交',request:99}]});assert.match(p.d.body.textContent,/发送结果不确定/);assert.match(p.d.body.textContent,/保留完整问题/);assert.equal(p.d.querySelector('[data-send-retry]'),null);assert.equal(p.calls.filter(c=>c.command==='result_retry_send').length,0)}finally{p.close()}});
