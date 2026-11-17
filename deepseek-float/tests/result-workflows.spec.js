import {test,expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
const html=readFileSync(new URL('../poc/result.html',import.meta.url),'utf8');
async function setup(page){
 await page.setViewportSize({width:400,height:560});
 await page.route('**/*',route=>route.abort());
 await page.setContent(html.replace('<head>','<head><script>window.__calls=[];window.__longFavorite=false;window.__failExport=false;window.__TAURI_INTERNALS__={invoke:(command,args)=>{__calls.push({command,args});if(command==="favorites_list")return Promise.resolve(window.__favoriteItems || (window.__longFavorite?[{id:2,title:"超长收藏标题".repeat(12),question:"超长问题".repeat(40),answer_markdown:"```text\\n"+"verylongunbrokencode".repeat(18)+"\\n```",source:"本地夹具",created_at:1}]:[{id:1,title:"本地收藏",question:"问题",answer_markdown:"收藏答案",source:"本地夹具",created_at:1}]));if(command==="result_export")return window.__failExport?Promise.reject("磁盘写入失败"):Promise.resolve("D:/"+"很长的导出目录/".repeat(20)+"结果.md");return Promise.resolve()}};</script>'));
 await page.evaluate(()=>__dsfResultSet({revision:7,title:'很长的结果标题'.repeat(12),turns:[{quote:'问题',text:'<p>已完成的回答</p>',complete:true,streaming:false,sendState:'complete'}]}));
}
test('result actions remain clickable within 400px card',async({page})=>{
 await setup(page);
 const boxes=await page.locator('.winbtns button').evaluateAll(nodes=>nodes.map(n=>{const b=n.getBoundingClientRect();return {left:b.left,right:b.right,width:b.width}}));
 expect(boxes).toHaveLength(5);expect(boxes.every(b=>b.left>=0&&b.right<=400&&b.width>0)).toBeTruthy();
 await expect(page.locator('[data-favorite]')).toBeVisible();await expect(page.locator('[data-export]')).toBeVisible();
});

test('question composer fits narrow cards and preserves keyboard sending',async({page})=>{
 await setup(page);
 const input=page.locator('#askInput');
 const send=page.locator('#askSend');
 await expect(send).toBeDisabled();
 for(const width of [320,400,480]){
  await page.setViewportSize({width,height:560});
  await input.fill('继续提问\n第二行内容');
  await input.focus();
  const bounds=await page.locator('.composer').boundingBox();
  const button=await send.boundingBox();
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.x+bounds.width).toBeLessThanOrEqual(width);
  expect(button.x+button.width).toBeLessThanOrEqual(bounds.x+bounds.width);
  expect(button.height).toBeGreaterThanOrEqual(28);
  expect(await page.locator('.composer').evaluate(el=>getComputedStyle(el).boxShadow)).not.toBe('none');
  await page.screenshot({path:`.test-artifacts/first-batch/composer-ui-${width}.png`,fullPage:true});
 }
 await input.fill('继续提问');
 await input.press('Shift+Enter');
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='result_ask').length)).toBe(0);
 await input.press('Enter');
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='result_ask').map(c=>c.args.text))).toEqual(['继续提问']);
 await expect(input).toHaveValue('');
 await expect(send).toBeDisabled();
});

test('single-line caret and send arrow remain vertically aligned after repeated typing',async({page})=>{
 await setup(page);
 const input=page.locator('#askInput');
 for(const value of ['', '提', '提问', '提问内容', '']){
  await input.fill(value);
  await input.focus();
  const geometry=await page.evaluate(()=>{
   const input=document.getElementById('askInput');
   const rect=input.getBoundingClientRect();
   const css=getComputedStyle(input);
   const button=document.getElementById('askSend').getBoundingClientRect();
   return {
    lineCenter:rect.top+parseFloat(css.borderTopWidth)+parseFloat(css.paddingTop)+parseFloat(css.lineHeight)/2,
    arrowCenter:button.top+button.height/2,
    height:rect.height,
   };
  });
  expect(Math.abs(geometry.lineCenter-geometry.arrowCenter)).toBeLessThanOrEqual(1);
  expect(geometry.height).toBe(30);
 }
 await page.screenshot({path:'.test-artifacts/first-batch/composer-alignment.png',fullPage:true});
});
test('library can return to the same conversation',async({page})=>{
 await setup(page);await page.locator('#btnFavorites').click();
 await expect(page.locator('#favoritesView')).toContainText('收藏答案');await expect(page.locator('#turns')).toBeHidden();
 await page.locator('#btnFavorites').click();await expect(page.locator('#turns')).toContainText('已完成的回答');await expect(page.locator('#composerWrap')).toBeVisible();
});
test('long export path is ellipsized and offers opening the directory',async({page})=>{
 await setup(page);await page.locator('[data-export]').click();
 await expect(page.locator('#saveStatus')).toContainText('结果.md');await expect(page.locator('#saveStatus button')).toBeVisible();
 await expect(page.locator('#saveStatus')).toHaveAttribute('data-state','success');
 await expect(page.locator('[data-export-filename]')).toHaveText('结果.md');
 await expect(page.locator('[data-export-path]')).toHaveAttribute('title',/D:\//);
 expect(await page.locator('[data-export-path]').evaluate(el=>getComputedStyle(el).textOverflow)).toBe('ellipsis');
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(400);
 await page.locator('#saveStatus button').click();expect(await page.evaluate(()=>__calls.some(c=>c.command==='result_open_exports_dir'))).toBeTruthy();
});
test('export failure uses the compact error state without losing retryable export action',async({page})=>{
 await setup(page);await page.evaluate(()=>window.__failExport=true);
 await page.locator('[data-export]').click();
 await expect(page.locator('#saveStatus')).toHaveAttribute('data-state','error');
 await expect(page.locator('#saveStatus')).toContainText('磁盘写入失败');
 await expect(page.locator('[data-export-filename]')).toHaveCount(0);
 await expect(page.locator('#saveStatus button')).toHaveCount(0);
});

test('favorites toolbar and long content stay within narrow result-card widths',async({page})=>{
 await setup(page);
 await page.evaluate(()=>window.__longFavorite=true);
 await page.locator('#btnFavorites').click();
 await page.locator('[data-favorite-expand]').click();
 for(const width of [320,400,480]){
  await page.setViewportSize({width,height:560});
  const layout=await page.evaluate(()=>{
   const root=document.documentElement;
   const search=document.querySelector('#favorites-search').getBoundingClientRect();
   const clear=document.querySelector('#favorites-search-clear').getBoundingClientRect();
   const count=document.querySelector('#favorites-count').getBoundingClientRect();
   const tools=document.querySelector('.favorites-tools').getBoundingClientRect();
   const meta=document.querySelector('.favorites-meta-row').getBoundingClientRect();
   const card=document.querySelector('#card').getBoundingClientRect();
   const title=document.querySelector('.favorite-title').getBoundingClientRect();
   const code=document.querySelector('.favorite-answer pre').getBoundingClientRect();
   const actionButtons=Array.from(document.querySelectorAll('.favorite-actions button')).map(button=>button.getBoundingClientRect());
   return {scroll:root.scrollWidth,client:root.clientWidth,search,clear,count,tools,meta,card,title,code,actionButtons};
  });
  expect(layout.scroll).toBeLessThanOrEqual(layout.client);
  expect(layout.search.width).toBeGreaterThan(100);
  expect(layout.clear.width).toBeGreaterThan(40);
  expect(layout.meta.top).toBeGreaterThanOrEqual(layout.search.bottom);
  expect(layout.count.left).toBeGreaterThanOrEqual(layout.tools.left);
  expect(layout.tools.right).toBeLessThanOrEqual(layout.card.right+1);
  expect(layout.title.right).toBeLessThanOrEqual(layout.card.right+1);
  expect(layout.code.right).toBeLessThanOrEqual(layout.card.right+1);
  expect(layout.actionButtons.every(button=>button.right<=layout.card.right+1)).toBeTruthy();
  await page.screenshot({path:`.test-artifacts/first-batch/favorites-ui-${width}.png`,fullPage:true});
 }
});

test('favorite search preserves the compact card layout and can return to the conversation',async({page})=>{
 await setup(page);await page.locator('#btnFavorites').click();
 await page.locator('#favorites-search').fill('不存在');await expect(page.locator('#favoritesView')).toContainText('没有匹配');
 await page.locator('#favorites-search-clear').click();await expect(page.locator('#favoritesView')).toContainText('收藏答案');
 expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(400);
 await page.screenshot({path:'.test-artifacts/first-batch/favorites-search.png',fullPage:true});
 await page.locator('#btnFavorites').click();await expect(page.locator('#composerWrap')).toBeVisible();
});

async function syntheticFavorites(page){
 await setup(page);
 await page.evaluate(()=>{
  const body='# 收藏标题\n\n**重要摘要**\n\n```js\nconst value = 1;\n```\n\n'+Array.from({length:18},(_,i)=>'正文第'+i+'行').join('\n')+'\n仅在全文末尾的 Needle';
  window.__favoriteItems=Array.from({length:18},(_,i)=>({id:i+1,kind:i===1?'selection':'answer',title:'测试收藏 '+(i+1),question:'测试问题',answer_markdown:i===1?'<b>原文标签</b>\n**原文星号**\n`原文代码`\n[链接](https://example.com)\n'+body:body,source:'合成夹具',created_at:100+i}));
 });
 await page.locator('#btnFavorites').click();
 await expect(page.locator('[data-favorite-id]')).toHaveCount(18);
}

test('favorites show four plain lines until expanded and copy and export the full saved content',async({page})=>{
 await syntheticFavorites(page);
 const entry=page.locator('[data-favorite-id="18"]');
 const summary=entry.locator('.favorite-summary');
 const geometry=await summary.evaluate(node=>({height:node.getBoundingClientRect().height,line:parseFloat(getComputedStyle(node).lineHeight),clamp:getComputedStyle(node).webkitLineClamp,children:node.children.length}));
 expect(geometry.clamp).toBe('4');expect(geometry.height).toBeGreaterThan(geometry.line*3);expect(geometry.height).toBeLessThanOrEqual(geometry.line*4+1);expect(geometry.children).toBe(0);
 await expect(entry.locator('.favorite-answer')).toHaveCount(0);
 await entry.locator('[data-favorite-copy]').click();await entry.locator('[data-favorite-export]').click();
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='result_copy').at(-1).args.text)).toContain('仅在全文末尾的 Needle');
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='favorites_export').at(-1).args.id)).toBe(18);
 await entry.locator('[data-favorite-expand]').click();
 await expect(entry.locator('.favorite-answer strong')).toHaveText('重要摘要');
 await expect(entry.locator('.favorite-answer pre code')).toHaveText('const value = 1;');
 await page.locator('#favorites-type').selectOption('selection');
 const raw=page.locator('[data-favorite-id="2"]');
 await raw.locator('[data-favorite-expand]').click();
 await expect(raw.locator('.favorite-answer')).toContainText('<b>原文标签</b>\n**原文星号**');
 expect(await raw.locator('.favorite-answer').evaluate(node=>node.children.length)).toBe(0);
 await raw.locator('[data-favorite-copy]').click();
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='result_copy').at(-1).args.text)).toContain('<b>原文标签</b>\n**原文星号**');
});

test('favorites toolbar stays fixed while refresh, background streams and confirmed deletion retain browsing state',async({page})=>{
 await syntheticFavorites(page);
 await expect(page.locator('#favorites-enlarge')).toBeVisible();
 await expect(page.locator('#favorites-enlarge')).toHaveAttribute('aria-pressed','false');
 await page.locator('#favorites-search').fill('Needle');
 await page.locator('#favorites-type').selectOption('answer');
 await expect(page.locator('#favorites-count')).toHaveText('17 / 18 条');
 const entry=page.locator('[data-favorite-id="16"]');
 await entry.locator('[data-favorite-expand]').click();
 const target=page.locator('[data-favorite-id="17"]');
 await target.locator('[data-favorite-delete]').click();
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='favorites_delete').length)).toBe(0);
 await target.locator('[data-favorite-cancel-delete]').click();
 await target.locator('[data-favorite-delete]').click();
 const before=await page.evaluate(()=>{
  const thread=document.querySelector('.thread');thread.scrollTop+=48;
  return {scroll:thread.scrollTop,tools:document.querySelector('.favorites-tools').getBoundingClientRect().top,thread:thread.getBoundingClientRect().top};
 });
 expect(before.scroll).toBeGreaterThan(0);expect(Math.abs(before.tools-before.thread)).toBeLessThanOrEqual(3);
 await page.evaluate(()=>{__dsfFavoritesRefresh();__dsfResultStream({revision:7,text:'后台更新',streaming:true});__dsfResultSet({revision:7,reset:false,turns:[{text:'后台完整结果',complete:true}]});});
 await expect(page.locator('#favorites-items')).not.toHaveAttribute('aria-busy','true');
 await expect(page.locator('#favorites-search')).toHaveValue('Needle');await expect(page.locator('#favorites-type')).toHaveValue('answer');
 await expect(entry.locator('[data-favorite-expand]')).toHaveAttribute('aria-expanded','true');
 expect(await page.locator('.thread').evaluate(node=>node.scrollTop)).toBeCloseTo(before.scroll,0);
 await target.locator('[data-favorite-confirm-delete]').evaluate(button=>button.click());
 await expect(target).toHaveCount(0);await expect(page.locator('#favorites-count')).toHaveText('16 / 17 条');
 await expect(entry.locator('[data-favorite-expand]')).toHaveAttribute('aria-expanded','true');
 expect(await page.locator('.thread').evaluate(node=>node.scrollTop)).toBeCloseTo(before.scroll,0);
 await expect(page.locator('#favorites-search')).toHaveValue('Needle');await expect(page.locator('#favorites-type')).toHaveValue('answer');
 await page.locator('#favorites-enlarge').click();await expect(page.locator('#favorites-enlarge')).toHaveText('还原');
 await expect(page.locator('#favorites-enlarge')).toHaveAttribute('aria-pressed','true');
 await page.locator('#favorites-enlarge').click();await expect(page.locator('#favorites-enlarge')).toHaveText('放大');
 await expect(page.locator('#favorites-enlarge')).toHaveAttribute('aria-pressed','false');
 await page.evaluate(()=>__dsfResultSet({revision:8,reset:true,turns:[{text:'显式新结果',complete:true}]}));
 await expect(page.locator('#favoritesView')).toBeHidden();await expect(page.locator('#turns')).toContainText('显式新结果');
 expect(await page.evaluate(()=>__calls.filter(c=>c.command==='favorites_set_view').map(c=>c.args))).toEqual([{active:true,expanded:false},{active:true,expanded:true},{active:true,expanded:false},{active:false,expanded:false}]);
});
