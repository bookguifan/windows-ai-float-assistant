import test from 'node:test';
import assert from 'node:assert/strict';
test('inline code beginning with an asterisk stays code in favorites',async()=>{const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:11,title:'代码',question:'',answer_markdown:'`*value*`',source:'',created_at:1}]:'ok'));try{p.d.getElementById('btnFavorites').click();await tick();assert.equal(p.d.querySelector('#favoritesView code')?.textContent,'*value*');}finally{p.close();}});

test('open favorites refreshes from the restored store and retains the search query',async()=>{
 let reads=0;
 const p=page(c=>Promise.resolve(c==='favorites_list'?++reads===1?[
  {id:1,title:'旧收藏',question:'问题',answer_markdown:'可搜索的答案',source:'',created_at:1},
 ]:[
  {id:1,title:'旧收藏',question:'问题',answer_markdown:'可搜索的答案',source:'',created_at:1},
  {id:2,title:'恢复的新收藏',question:'问题',answer_markdown:'可搜索的新答案',source:'',created_at:2},
 ]:'ok'));
 try {
  p.d.getElementById('btnFavorites').click();await tick();
  const search=p.d.getElementById('favorites-search');search.value='可搜索';search.dispatchEvent(new p.w.Event('input'));
  p.w.__dsfFavoritesRefresh();await tick();
  assert.equal(reads,2);
  assert.equal(p.d.querySelectorAll('#favoritesView [data-favorite-copy]').length,2);
  assert.equal(p.d.getElementById('favorites-search').value,'可搜索');
  assert.match(p.d.getElementById('favoritesView').textContent,/恢复的新收藏/);
 }finally{p.close();}
});
test('refreshing favorites renders each matching item once and retains the search query',async()=>{
 const items=[
  {id:1,title:'匹配收藏甲',question:'问题',answer_markdown:'答案甲',source:'',created_at:1},
  {id:2,title:'匹配收藏乙',question:'问题',answer_markdown:'答案乙',source:'',created_at:2},
  {id:3,title:'其他收藏',question:'问题',answer_markdown:'答案丙',source:'',created_at:3},
 ];
 const p=page(c=>Promise.resolve(c==='favorites_list'?items:'ok'));
 try {
  p.d.getElementById('btnFavorites').click();await tick();
  const search=p.d.getElementById('favorites-search');
  search.value='匹配收藏';search.dispatchEvent(new p.w.Event('input'));
  let appended=0;
  const original=p.w.Element.prototype.appendChild;
  p.w.Element.prototype.appendChild=function(child){
   if(this.id==='favorites-items')appended++;
   return original.call(this,child);
  };
  p.w.__dsfFavoritesRefresh();await tick();
  assert.equal(search.value,'匹配收藏');
  assert.equal(p.d.querySelectorAll('#favorites-items [data-favorite-copy]').length,2);
  assert.equal(appended,2,'each matching favorite should be appended only once per refresh');
 }finally{p.close();}
});
import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
function page(handler=()=>Promise.resolve('ok')){const html=readFileSync(new URL('../poc/result.html',import.meta.url),'utf8');const dom=new JSDOM(html,{runScripts:'outside-only',url:'https://tauri.localhost'});const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(c,a)=>{calls.push({c,a});return handler(c,a);}};dom.window.ResizeObserver=class{observe(){}};for(const s of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi))dom.window.eval(s[1]);return {w:dom.window,d:dom.window.document,calls,close:()=>dom.window.close()};}
const tick=()=>new Promise(r=>setTimeout(r,0));

test('menu opens favorites directly without toggling it closed or losing its search',async()=>{
 const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:1,title:'保留收藏',question:'',answer_markdown:'答案',source:'',created_at:1}]:'ok'));
 try {
  assert.equal(typeof p.w.__dsfFavoritesOpen,'function');
  p.w.__dsfFavoritesOpen(); await tick();
  assert.equal(p.d.getElementById('favoritesView').hidden,false);
  const search=p.d.getElementById('favorites-search');search.value='保留';search.dispatchEvent(new p.w.Event('input'));
  p.w.__dsfFavoritesOpen(); await tick();
  assert.equal(p.d.getElementById('favoritesView').hidden,false);
  assert.equal(p.d.getElementById('favorites-search').value,'保留');
  assert.match(p.d.getElementById('favoritesView').textContent,/保留收藏/);
  p.d.getElementById('btnFavorites').click();
  assert.equal(p.d.getElementById('favoritesView').hidden,true);
 }finally{p.close();}
});
test('favorites search matches title question and body, ignores case, and clearing restores results',async()=>{
 const items=[{id:1,title:'标题甲',question:'问题甲',answer_markdown:'Rust 示例',source:'',created_at:1},{id:2,title:'标题乙',question:'特别问题',answer_markdown:'另一答案',source:'',created_at:2}];
 const p=page(c=>Promise.resolve(c==='favorites_list'?items:'ok'));
 try {
  p.d.getElementById('btnFavorites').click();await tick();
  const search=p.d.getElementById('favorites-search');assert.ok(search,'search input is present');
  for(const query of ['  rUsT  ','标题甲','问题甲']){search.value=query;search.dispatchEvent(new p.w.Event('input'));assert.equal(p.d.querySelectorAll('#favoritesView [data-favorite-copy]').length,1);assert.match(p.d.getElementById('favoritesView').textContent,/标题甲/);}
  search.value='不存在';search.dispatchEvent(new p.w.Event('input'));assert.match(p.d.getElementById('favoritesView').textContent,/没有匹配/);
  p.d.getElementById('favorites-search-clear').click();assert.equal(search.value,'');assert.equal(p.d.querySelectorAll('#favoritesView [data-favorite-copy]').length,2);
 }finally{p.close();}
});
test('deleting a filtered favorite updates the source list and does not resurrect it when clearing search',async()=>{
 const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:1,title:'甲',question:'',answer_markdown:'删除目标',source:'',created_at:1},{id:2,title:'乙',question:'',answer_markdown:'保留目标',source:'',created_at:1}]:'ok'));
 try{
  p.d.getElementById('btnFavorites').click();await tick();const search=p.d.getElementById('favorites-search');assert.ok(search);
  search.value='删除';search.dispatchEvent(new p.w.Event('input'));p.d.querySelector('[data-favorite-delete]').click();await tick();
  assert.match(p.d.getElementById('favoritesView').textContent,/没有匹配/);
  p.d.getElementById('favorites-search-clear').click();assert.equal(p.d.querySelectorAll('[data-favorite-copy]').length,1);assert.doesNotMatch(p.d.getElementById('favoritesView').textContent,/删除目标/);
 }finally{p.close();}
});
test('a pending deletion stays disabled after search redraws the same favorite',async()=>{
 let finish;
 const p=page(c=>c==='favorites_delete'?new Promise(resolve=>{finish=resolve}):Promise.resolve(c==='favorites_list'?[{id:1,title:'甲',question:'',answer_markdown:'目标',source:'',created_at:1}]:'ok'));
 try{
  p.d.getElementById('btnFavorites').click();await tick();p.d.querySelector('[data-favorite-delete]').click();
  const search=p.d.getElementById('favorites-search');search.value='目标';search.dispatchEvent(new p.w.Event('input'));
  const button=p.d.querySelector('[data-favorite-delete]');assert.equal(button.disabled,true);button.click();
  assert.equal(p.calls.filter(c=>c.c==='favorites_delete').length,1);finish();await tick();
  assert.match(p.d.getElementById('favoritesView').textContent,/暂无收藏/);
 }finally{p.close();}
});
test('favorite captures only answer Markdown with session identity',async()=>{const p=page();try{p.w.__dsfResultSet({revision:7,title:'标题',turns:[{quote:'问题',text:'<ul><li>项目</li></ul><table><tr><td>a|b</td></tr></table><pre><code>  a\n```</code></pre>',thinking:'秘密思考'}]});p.d.querySelector('[data-favorite]').click();await tick();const call=p.calls.find(c=>c.c==='result_favorite_save');assert.equal(call.a.revision,7);assert.equal(call.a.turn.index,0);assert.match(call.a.turn.markdown,/- 项目/);assert.doesNotMatch(call.a.turn.markdown,/- - /);assert.match(call.a.turn.markdown,/a\\\|b/);assert.match(call.a.turn.markdown,/````\n  a\n```\n````/);assert.doesNotMatch(call.a.turn.markdown,/秘密思考|收藏|复制/);}finally{p.close();}});
test('export current and conversation includes ordered answer snapshots',async()=>{const p=page();try{p.w.__dsfResultSet({revision:5,turns:[{quote:'一',text:'答案一',complete:true},{quote:'二',text:'答案二',complete:true}]});p.d.querySelector('[data-export]').click();p.d.getElementById('btnExport').click();await tick();const calls=p.calls.filter(c=>c.c==='result_export');assert.deepEqual(calls.map(c=>c.a.turns.length),[1,2]);assert.equal(calls[1].a.whole,true);assert.equal(calls[1].a.turns[1].index,1);}finally{p.close();}});
test('favorite errors remain visible and browsing supports delete/copy/export',async()=>{const p=page((c)=>c==='result_favorite_save'?Promise.reject('磁盘写入失败'):Promise.resolve(c==='favorites_list'?[{id:3,title:'收藏标题',question:'问题',answer_markdown:'答案',source:'站点',created_at:1}]:'ok'));try{p.w.__dsfResultSet({revision:1,turns:[{text:'答案'}]});p.d.querySelector('[data-favorite]').click();await tick();assert.match(p.d.body.textContent,/磁盘写入失败/);p.d.getElementById('btnFavorites').click();await tick();assert.match(p.d.body.textContent,/收藏标题/);p.d.querySelector('[data-favorite-copy]').click();p.d.querySelector('[data-favorite-export]').click();p.d.querySelector('[data-favorite-delete]').click();await tick();assert.ok(p.calls.some(c=>c.c==='favorites_delete'));assert.ok(p.calls.some(c=>c.c==='favorites_export'));assert.ok(p.calls.some(c=>c.c==='result_copy'&&c.a.text==='答案'));}finally{p.close();}});

test('detached old answer buttons keep the original session identity',async()=>{const p=page();try{p.w.__dsfResultSet({revision:5,turns:[{quote:'旧问题',text:'答案'}]});const old=p.d.querySelector('[data-favorite]');p.w.__dsfResultSet({revision:6,turns:[{quote:'新问题',text:'答案'}]});old.click();await tick();assert.equal(p.calls.find(c=>c.c==='result_favorite_save').a.revision,5);}finally{p.close();}});
test('whole export rejects incomplete or streaming answers while completed history remains available',()=>{const p=page();try{p.w.__dsfResultSet({revision:5,turns:[{text:'完成',complete:true},{text:'部分',complete:false,streaming:false}]});p.d.getElementById('btnExport').click();assert.equal(p.calls.filter(c=>c.c==='result_export').length,0);assert.match(p.d.body.textContent,/等待所有回答完成/);assert.ok(p.d.querySelector('[data-favorite]'));}finally{p.close();}});
test('favorite view preserves escaped table pipes and long code fences',async()=>{const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:8,title:'格式',question:'问题',answer_markdown:'| a\\|b |\n| --- |\n\n````\n  a\n```\n````',source:'本地',created_at:1}]:'ok'));try{p.d.getElementById('btnFavorites').click();await tick();assert.equal(p.d.querySelector('#favoritesView th').textContent,'a|b');assert.equal(p.d.querySelector('#favoritesView pre code').textContent,'  a\n```');}finally{p.close();}});
test('nested ordered and unordered lists keep hierarchy, continuation lines and start numbers',async()=>{const p=page();try{p.w.__dsfResultSet({revision:2,turns:[{text:'<ol start="3"><li>父项<br>续行<ul><li>子项<ol start="7"><li>孙项</li></ol></li></ul></li><li>下一项</li></ol>'}]});p.d.querySelector('[data-favorite]').click();await tick();const md=p.calls.find(c=>c.c==='result_favorite_save').a.turn.markdown;assert.match(md,/3\. 父项\n   续行\n   - 子项\n     7\. 孙项\n4\. 下一项/);}finally{p.close();}});
test('inline code preserves literal backticks and edge spaces when saved and viewed',async()=>{let saved='';const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:9,title:'代码',question:'',answer_markdown:saved,source:'',created_at:1}]:'ok'));try{p.w.__dsfResultSet({revision:3,turns:[{text:'<p><code>a`b</code> <code>`edge`</code> <code> leading </code> <code>*value*</code></p>'}]});p.d.querySelector('[data-favorite]').click();await tick();saved=p.calls.find(c=>c.c==='result_favorite_save').a.turn.markdown;assert.match(saved,/``a`b``/);p.d.getElementById('btnFavorites').click();await tick();assert.deepEqual(Array.from(p.d.querySelectorAll('#favoritesView code'),c=>c.textContent),['a`b','`edge`',' leading ','*value*']);}finally{p.close();}});
test('favorite browsing renders saved nested lists with the same hierarchy',async()=>{let saved='';const p=page(c=>Promise.resolve(c==='favorites_list'?[{id:10,title:'列表',question:'',answer_markdown:saved,source:'',created_at:1}]:'ok'));try{p.w.__dsfResultSet({revision:4,turns:[{text:'<ol start="3"><li>父项<br>续行<ul><li>子项<ol start="7"><li>孙项</li></ol></li></ul></li><li>下一项</li></ol>'}]});p.d.querySelector('[data-favorite]').click();await tick();saved=p.calls.find(c=>c.c==='result_favorite_save').a.turn.markdown;p.d.getElementById('btnFavorites').click();await tick();assert.ok(p.d.querySelector('#favoritesView ol[start="3"] > li > ul > li > ol[start="7"]'));assert.equal(p.d.querySelector('#favoritesView ol').children.length,2);}finally{p.close();}});
