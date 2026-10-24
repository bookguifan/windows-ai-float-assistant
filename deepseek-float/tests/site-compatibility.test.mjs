import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';
const source=readFileSync(new URL('../src-tauri/src/dsfloat_init.js',import.meta.url),'utf8');
function page(url='https://fixture.test/'){
 const dom=new JSDOM('<main><div id="messages"></div><form><textarea></textarea></form></main>',{runScripts:'outside-only',url});
 const w=dom.window,d=w.document;
 w.HTMLElement.prototype.getBoundingClientRect=function(){return {width:600,height:40,top:this.tagName==='TEXTAREA'||this.tagName==='BUTTON'?700:100,bottom:this.tagName==='TEXTAREA'||this.tagName==='BUTTON'?740:140,left:0,right:600}};
 w.eval(source.replace('  const api = {','  const api = {splitBlock,tailHasEcho,findFileInput,'));
 const api=w.__DSFLOAT__,messages=d.querySelector('#messages');
 function message(role,text){const el=d.createElement('div');el.dataset.messageAuthorRole=role;const p=d.createElement('p');p.textContent=text;el.append(p);messages.append(el);return el}
 return {w,d,api,message,close:()=>w.close()};
}

test('ChatGPT prose word spans preserve authored newline breaks in result HTML', () => {
 const p=page('https://chatgpt.com/');
 try {
  const block=p.d.createElement('div');
  block.innerHTML='<p><span data-d-text-flow="word">First line\n\n</span><span data-d-text-flow="word">Second line</span></p>';
  const html=p.api.splitBlock(block).ans;
  const result=p.d.createElement('div');result.innerHTML=html;
  assert.equal(result.querySelectorAll('br').length,2);
  assert.match(html,/First line<br><br>/);
 } finally {p.close();}
});

test('ChatGPT attachment scope includes sibling upload controls and excludes camera capture', () => {
 const p=page('https://chatgpt.com/');
 try {
  p.d.querySelector('form').outerHTML='<div data-composer-surface-variant="default"><div id="attachments"><img src="blob:old"></div><div data-composer-body><textarea></textarea><button aria-label="Send">发送</button></div><input type="file" accept="image/*" capture="environment"><input id="photos" type="file" accept="image/*" multiple></div>';
  assert.equal(p.api.findFileInput()?.id,'photos');
  p.api.shotReset();assert.equal(p.api.hasAttachment(),false);
  p.d.querySelector('#messages').innerHTML='<img src="blob:history">';
  assert.equal(p.api.hasAttachment(),false);
  p.d.querySelector('#attachments').insertAdjacentHTML('beforeend','<img src="blob:new">');
  assert.equal(p.api.hasAttachment(),true);
 } finally {p.close();}
});

test('DeepSeek attachment sibling is scoped separately from the send controls', () => {
 const p=page('https://chat.deepseek.com/');
 try {
  p.d.querySelector('form').outerHTML='<div id="composer"><div id="attachments"><img src="blob:existing"></div><div id="input"><textarea></textarea><button aria-label="Send">发送</button><input type="file" accept="image/png"></div></div>';
  p.api.shotReset();assert.equal(p.api.hasAttachment(),false);
  p.d.querySelector('#messages').innerHTML='<img src="blob:history">';
  assert.equal(p.api.hasAttachment(),false);
  p.d.querySelector('#attachments').insertAdjacentHTML('beforeend','<img src="blob:new-image">');
  assert.equal(p.api.hasAttachment(),true,'new sibling attachment is visible inside composer');
  p.d.querySelector('#attachments').insertAdjacentHTML('beforeend','<div class="ds-loading"></div>');
  assert.equal(p.api.hasAttachment(),false,'upload spinner must block OCR submission');
  p.d.querySelector('.ds-loading').remove();assert.equal(p.api.hasAttachment(),true);
 } finally { p.close(); }
});

test('ChatGPT model completion cannot finish an answer while its renderer still reveals content', () => {
 const p=page('https://chatgpt.com/');
 try {
  p.w.__dsfloatRequest=90;p.api.prepareSend('隐藏面板问题');
  p.d.querySelector('#messages').innerHTML='<section data-talvt-turn-state="complete"><div data-user-message-bubble>隐藏面板问题</div><div data-dil-message-id="answer" data-d-streaming-reveal><h3>一、</h3></div></section>';
  let s=p.api.snapshotConversation('隐藏面板问题',false);
  assert.equal(s.requestBound,true);
  assert.equal(s.done,false,'generation completion does not prove DOM rendering completion');
  const answer=p.d.querySelector('[data-dil-message-id]');
  answer.innerHTML='<h3>一、列表</h3><ul><li>完整回答</li></ul><p>END</p>';
  answer.removeAttribute('data-d-streaming-reveal');
  s=p.api.snapshotConversation('隐藏面板问题',false);
  assert.equal(s.done,true);assert.match(s.texts.at(-1),/END/);
 } finally { p.close(); }
});

test('ChatGPT virtualized code with omitted lines cannot be declared complete', () => {
 const p=page('https://chatgpt.com/');
 try {
  p.w.__dsfloatRequest=91;p.api.prepareSend('长代码问题');
  p.d.querySelector('#messages').innerHTML='<section data-talvt-turn-state="complete"><div data-user-message-bubble>长代码问题</div><div data-dil-message-id="answer"><div data-code-block="true"><div class="cm-content"><div class="cm-line">first line</div><div class="cm-gap" style="height:1000px"></div><div class="cm-line">last line</div></div></div></div></section>';
  assert.equal(p.api.snapshotConversation('长代码问题',false).done,false);
 } finally { p.close(); }
});

test('ChatGPT appended second turn accepts an identical answer completed between polls', () => {
 const p=page('https://chatgpt.com/');
 try {
  const messages=p.d.querySelector('#messages');
  const turn='<section data-talvt-turn-state="complete"><div data-user-message-bubble>计算17+25</div><div data-dil-message-id="first"><p>42</p></div></section>';
  messages.innerHTML=turn;
  p.w.__dsfloatRequest=81;
  p.api.prepareSend('计算17+25');
  assert.equal(p.api.sendEvidence('计算17+25'),'unknown');
  messages.insertAdjacentHTML('beforeend',turn.replace('"first"','"second"'));
  const s=p.api.snapshotConversation('计算17+25',false);
  assert.equal(s.requestBound,true);
  assert.equal(s.texts.length,2);
  assert.match(s.texts[1],/42/);
  assert.equal(s.done,true);
 } finally { p.close(); }
});

test('ChatGPT CodeMirror answer preserves code lines and excludes the code toolbar', () => {
 const p=page('https://chatgpt.com/');
 try {
  p.w.__dsfloatRequest=83;
  p.api.prepareSend('输出代码');
  p.d.querySelector('#messages').innerHTML='<section data-talvt-turn-state="complete"><div data-user-message-bubble>输出代码</div><div data-dil-message-id="code-answer"><div data-code-block="true"><div data-markdown-copy="exclude">Python<button>运行</button></div><div class="cm-announced">editor announcement</div><div class="cm-content" data-language="python"><div class="cm-line">if 1 &lt; 2:</div><div class="cm-line">  print(&quot;A &amp; B&quot;)</div><div class="cm-line"><br></div><div class="cm-line"># end</div></div></div></div></section>';
  const s=p.api.snapshotConversation('输出代码',false);
  const out=p.d.createElement('div');out.innerHTML=s.texts.at(-1);
  const code=out.querySelector('pre code');
  assert.ok(code,'observed code editor becomes a semantic code block');
  assert.equal(code.className,'language-python');
  assert.equal(code.textContent,'if 1 < 2:\n  print("A & B")\n\n# end');
  assert.doesNotMatch(s.texts.at(-1),/运行|editor announcement/);
  assert.equal(s.done,true);
 } finally { p.close(); }
});

test('CodeMirror conversion is limited to explicit ChatGPT answer code blocks', () => {
 for(const url of ['https://fixture.test/','https://chatgpt.com/']) {
  const p=page(url);
  try {
   const el=p.d.createElement('div');
   if(url.includes('fixture')) el.setAttribute('data-code-block','true');
   el.innerHTML='<div class="cm-content" data-language="python"><div class="cm-line">ordinary prose</div></div>';
   assert.doesNotMatch(p.api.splitBlock(el).ans,/<pre>/);
  } finally { p.close(); }
 }
});

test('appending a question cannot reuse an old assistant node as its identical answer', () => {
 const p=page('https://chatgpt.com/');
 try {
  const messages=p.d.querySelector('#messages');
  messages.innerHTML='<section data-talvt-turn-state="complete"><div data-user-message-bubble>计算17+25</div><div data-dil-message-id="first"><p>42</p></div></section>';
  const oldAnswer=messages.querySelector('[data-dil-message-id]');
  p.w.__dsfloatRequest=82;
  p.api.prepareSend('计算17+25');
  messages.insertAdjacentHTML('beforeend','<section data-talvt-turn-state="complete"><div data-user-message-bubble>计算17+25</div></section>');
  assert.equal(p.api.snapshotConversation('计算17+25',false).texts.length,1);
  messages.lastChild.append(oldAnswer);
  const s=p.api.snapshotConversation('计算17+25',false);
  assert.equal(s.texts.length,1);
  assert.equal(s.done,false);
 } finally { p.close(); }
});
test('upload button is never a send candidate',()=>{const p=page();try{p.d.querySelector('textarea').value='问题';p.d.querySelector('form').insertAdjacentHTML('beforeend','<button type="button">上传图片</button>');let clicks=0;p.d.querySelector('button').onclick=()=>clicks++;assert.equal(p.api.submit('问题',0),'no-send-button');assert.equal(clicks,0)}finally{p.close()}});
test('missing composer is unknown, not proof of a sent message',()=>{const p=page();try{p.d.querySelector('textarea').remove();assert.equal(p.api.sendEvidence('问题'),'unknown')}finally{p.close()}});
test('submission is single-use even when the editor clears late',()=>{const p=page();try{p.w.__dsfloatRequest=10;p.api.prepareSend('问题');p.d.querySelector('textarea').value='问题';p.d.querySelector('form').insertAdjacentHTML('beforeend','<button type="button" aria-label="Send">发送</button>');let clicks=0;p.d.querySelector('button').onclick=()=>clicks++;assert.equal(p.api.submit('问题',0),'ok');assert.equal(p.api.submit('问题',0),'already-submitted');assert.equal(clicks,1);assert.equal(p.api.sendEvidence('问题'),'unknown')}finally{p.close()}});
test('mismatching draft is not submitted',()=>{const p=page();try{p.d.querySelector('textarea').value='错误的草稿';p.d.querySelector('form').insertAdjacentHTML('beforeend','<button aria-label="Send">发送</button>');assert.equal(p.api.submit('完整问题',0),'content-mismatch')}finally{p.close()}});
test('HTML entities preserve echo identity',()=>{const p=page();try{p.message('user','A & B < C');p.message('assistant','这是足够长的完整回答，用于验证特殊字符问题的会话定位。');assert.equal(p.api.snapshotConversation('A & B < C',false).ok,true)}finally{p.close()}});
test('repeated prompt never accepts unchanged old messages',()=>{const p=page();try{p.message('user','请说明测试闭环');p.message('assistant','上一轮已经完成的旧回答，这条内容不能用来交付本轮问题。');p.w.__dsfloatRequest=11;p.api.prepareSend('请说明测试闭环');const s=p.api.snapshotConversation('请说明测试闭环',false);assert.deepEqual(Array.from(s.texts),[]);assert.equal(s.done,false);assert.equal(p.api.sendEvidence('请说明测试闭环'),'unknown');p.message('user','请说明测试闭环');p.message('assistant','第二轮真正的新回答，必须绑定新增的用户消息。');assert.equal(p.api.sendEvidence('请说明测试闭环'),'sent');assert.match(p.api.snapshotConversation('请说明测试闭环',false).texts.at(-1),/第二轮真正/)}finally{p.close()}});
test('rebuilding old DOM is not a new question',()=>{const p=page();try{p.message('user','请说明测试闭环');p.message('assistant','这是之前的完整回答，DOM 重新挂载也不能将它变为新回答。');p.w.__dsfloatRequest=12;p.api.prepareSend('请说明测试闭环');const list=p.d.querySelector('#messages');list.innerHTML=list.innerHTML;assert.equal(p.api.sendEvidence('请说明测试闭环'),'unknown')}finally{p.close()}});
test('normal answer starting with Thinking is not a reasoning placeholder',()=>{const p=page();try{const el=p.message('assistant','Thinking through this example gives the result 42.');const part=p.api.splitBlock(el);assert.equal(part.pending,false);assert.match(part.ans,/Thinking/)}finally{p.close()}});
test('rendered prose alone does not prove generation is finished',()=>{const p=page();try{const el=p.message('assistant','生成中的正文已经出现，但还没有完成。');assert.equal(p.api.splitBlock(el).done,false);el.insertAdjacentHTML('beforeend','<button aria-label="Copy">复制</button>');assert.equal(p.api.splitBlock(el).done,true)}finally{p.close()}});
test('same text with changed markup invalidates the message cache',()=>{const p=page();try{const el=p.message('assistant','相同文字');el.firstChild.innerHTML='<em>相同文字</em>';p.api.splitBlock(el);el.firstChild.innerHTML='<strong>相同文字</strong>';assert.match(p.api.splitBlock(el).ans,/<strong>/)}finally{p.close()}});
test('reasoning-only block stays in the snapshot instead of disappearing',()=>{const p=page();try{p.message('user','请说明测试闭环');const el=p.message('assistant','这里是足够长的思考正文，还没有形成最终回答。');el.className='ds-think';const snap=p.api.snapshotConversation('请说明测试闭环',false);assert.equal(snap.pending,true);assert.match(snap.thinking.at(-1),/思考正文/)}finally{p.close()}});
test('PDF-only upload input is rejected',()=>{const p=page();try{p.d.querySelector('form').insertAdjacentHTML('beforeend','<input type="file" accept="application/pdf">');assert.equal(p.api.findFileInput(),null)}finally{p.close()}});
test('old message and old composer images are not a new attachment',()=>{const p=page();try{p.d.querySelector('#messages').innerHTML='<img src="blob:old-message">';p.d.querySelector('form').insertAdjacentHTML('beforeend','<img src="blob:old-composer">');p.api.shotReset();assert.equal(p.api.hasAttachment(),false);p.d.querySelector('form').insertAdjacentHTML('beforeend','<img src="blob:new-capture">');assert.equal(p.api.hasAttachment(),true)}finally{p.close()}});

test('old question remount plus changed old answer is not a new submission',()=>{const p=page();try{p.message('user','请说明测试闭环');p.message('assistant','旧回答正在改变，不能被当作本轮新增消息的证明。');p.w.__dsfloatRequest=13;p.api.prepareSend('请说明测试闭环');const list=p.d.querySelector('#messages');list.innerHTML=list.innerHTML;list.lastChild.firstChild.textContent+='旧回答尾段';assert.equal(p.api.sendEvidence('请说明测试闭环'),'unknown')}finally{p.close()}});
test('message node reorder preserves the bound current question',()=>{const p=page();try{p.message('user','旧问题');p.message('assistant','旧回答足够长，不能作为下一轮回答。');p.w.__dsfloatRequest=14;p.api.prepareSend('新问题');const q=p.message('user','新问题');const a=p.message('assistant','本轮回答的完整内容，移动同一节点不应中断定位。');assert.equal(p.api.sendEvidence('新问题'),'sent');a.remove();p.d.querySelector('#messages').append(a);assert.match(p.api.snapshotConversation('新问题',false).texts.at(-1),/本轮回答/)}finally{p.close()}});

test('confirmed question survives a full DOM remount without accepting old answer',()=>{const p=page();try{p.message('user','旧问题');p.message('assistant','上一轮的完整旧回答。');p.w.__dsfloatRequest=15;p.api.prepareSend('本轮问题');p.message('user','本轮问题');p.message('assistant','本轮开始回答，正在生成中。');assert.equal(p.api.sendEvidence('本轮问题'),'sent');const list=p.d.querySelector('#messages');list.innerHTML=list.innerHTML;list.lastChild.firstChild.textContent+='这里是重建后的流式尾段';assert.equal(p.api.sendEvidence('本轮问题'),'sent');assert.match(p.api.snapshotConversation('本轮问题',false).texts.at(-1),/重建后的流式尾段/)}finally{p.close()}});
test('virtual list reuses old node for a different new question',()=>{const p=page();try{const old=p.message('user','旧问题');const answer=p.message('assistant','上一轮答案，在重新绑定时不能交付。');p.w.__dsfloatRequest=16;p.api.prepareSend('完全不同的新问题');old.firstChild.textContent='完全不同的新问题';assert.equal(p.api.snapshotConversation('完全不同的新问题',false).texts.length,1);answer.firstChild.textContent='这次新回答的完整内容，节点被框架复用。';assert.equal(p.api.sendEvidence('完全不同的新问题'),'sent');assert.match(p.api.snapshotConversation('完全不同的新问题',false).texts.at(-1),/这次新回答/)}finally{p.close()}});

test('observed DeepSeek icon sender is accepted only on DeepSeek',()=>{for(const url of ['https://chat.deepseek.com/','https://fixture.test/']){const p=page(url);try{p.d.querySelector('textarea').value='测试发送';p.d.querySelector('form').insertAdjacentHTML('beforeend','<div role="button" class="ds-button--primary ds-button--circle"><svg><path d="M8.3125 V15.0417H7V3.95579 Z"></path></svg></div>');let clicks=0;p.d.querySelector('[role="button"]').onclick=()=>clicks++;assert.equal(p.api.submit('测试发送',0),url.includes('chat.deepseek.com')?'ok':'no-send-button');assert.equal(clicks,url.includes('chat.deepseek.com')?1:0)}finally{p.close()}}});

test('DeepSeek completion needs both observed answer action icons',()=>{for(const url of ['https://chat.deepseek.com/','https://fixture.test/']){const p=page(url);try{const el=p.message('assistant','这是一条真实正文，操作行出现后才允许完成。');el.firstChild.className='ds-markdown ds-assistant-message-main-content';el.insertAdjacentHTML('beforeend','<div role="button" class="ds-button"><svg><path d="M6.14929 4.02032 C1 2 3"></path></svg></div>');assert.equal(p.api.splitBlock(el).done,false);el.insertAdjacentHTML('beforeend','<div role="button" class="ds-button"><svg><path d="M7.92136 0.349152 C1 2 3"></path></svg></div>');assert.equal(p.api.splitBlock(el).done,url.includes('chat.deepseek.com'))}finally{p.close()}}});

test('rebuilt new question with unchanged old answer never delivers the old answer',()=>{const p=page();try{p.message('user','请说明上一轮测试闭环的完整步骤');const a=p.message('assistant','这是上一轮已经完成的回答，绝不能当成本轮问题的答案。');a.firstChild.insertAdjacentHTML('beforeend','<button aria-label="Copy">复制</button>');p.w.__dsfloatRequest=21;p.api.prepareSend('请说明本轮新问题的实施细节与结果');const list=p.d.querySelector('#messages');list.firstChild.firstChild.textContent='请说明本轮新问题的实施细节与结果';list.innerHTML=list.innerHTML;const snap=p.api.snapshotConversation('请说明本轮新问题的实施细节与结果',false);assert.equal(snap.texts.length,1);assert.equal(snap.done,false)}finally{p.close()}});
test('observed current answer keeps its identity when final text equals an old answer',()=>{const p=page();try{const q=p.message('user','上一题');const old='这是两道问题共享的正确标准答案。';const a=p.message('assistant',old);p.w.__dsfloatRequest=22;p.api.prepareSend('本轮新题');q.firstChild.textContent='本轮新题';assert.equal(p.api.snapshotConversation('本轮新题',false).texts.length,1);a.firstChild.textContent='这是两道问题共享的正确标准答案，仍在生成';assert.equal(p.api.snapshotConversation('本轮新题',false).texts.length,2);a.firstChild.textContent=old;assert.equal(p.api.snapshotConversation('本轮新题',false).texts.length,2)}finally{p.close()}});
test('ChatGPT semantic composer boundary includes nested send and excludes history resend',()=>{const p=page('https://chatgpt.com/');try{p.d.querySelector('form').remove();p.d.querySelector('main').insertAdjacentHTML('beforeend','<button aria-label="Resend">old</button><div data-composer-body><div><textarea></textarea><button aria-label="Upload">upload</button></div><button aria-label="Send">send</button></div>');p.d.querySelector('textarea').value='新问题';const clicks=[];p.d.querySelectorAll('button').forEach(b=>b.onclick=()=>clicks.push(b.textContent));assert.equal(p.api.submit('新问题',0),'ok');assert.deepEqual(clicks,['send'])}finally{p.close()}});
test('Kimi sender belongs to editor and never selects its SVG child',()=>{const p=page('https://www.kimi.com/');try{p.d.querySelector('form').setAttribute('data-testid','chat-editor');p.d.querySelector('textarea').value='新问题';p.d.querySelector('form').insertAdjacentHTML('beforeend','<div class="send-button-container"><svg class="send-icon" name="Send"></svg></div>');let clicks=0;p.d.querySelector('.send-button-container').onclick=()=>clicks++;assert.equal(p.api.submit('新问题',0),'ok');assert.equal(clicks,1)}finally{p.close()}});
test('Gemini reads only semantic query and answer, excluding disclaimer',()=>{const p=page('https://gemini.google.com/app');try{p.w.__dsfloatRequest=30;p.api.prepareSend('本轮测试问题');p.d.querySelector('#messages').innerHTML='<user-query><span>你说 本轮测试问题</span><p class="query-text-line">本轮测试问题</p></user-query><model-response><message-content><p>正确答案为四十二。</p></message-content><button aria-label="复制"></button><button aria-label="重做"></button></model-response><p>Gemini 是一款 AI 工具，其回答未必正确无误。</p>';const s=p.api.snapshotConversation('本轮测试问题',false);assert.equal(s.requestBound,true);assert.equal(s.done,true);assert.equal(s.texts.length,2);assert.doesNotMatch(s.texts.at(-1),/未必正确|你说/);assert.match(s.texts.at(-1),/四十二/)}finally{p.close()}});
test('Gemini unchanged previous round and unstructured prose cannot become a new answer',()=>{const p=page('https://gemini.google.com/app');try{p.d.querySelector('#messages').innerHTML='<user-query><p class="query-text-line">重复问题</p></user-query><model-response><message-content><p>上一轮的真实答案。</p></message-content></model-response>';p.w.__dsfloatRequest=31;p.api.prepareSend('重复问题');p.d.querySelector('#messages').innerHTML=p.d.querySelector('#messages').innerHTML;assert.equal(p.api.snapshotConversation('重复问题',false).texts.length,0);assert.equal(p.api.sendEvidence('重复问题'),'unknown')}finally{p.close()}});
test('ChatGPT current semantic turns exclude disclaimer and require complete state',()=>{const p=page('https://chatgpt.com/');try{p.w.__dsfloatRequest=32;p.api.prepareSend('本轮提问');p.d.querySelector('#messages').innerHTML='<div data-talvt-turn-state="streaming"><div data-user-message-bubble>本轮提问</div><div data-dil-message-id="answer"><p>这是本轮答案</p></div></div><p>ChatGPT 可能会出错。请核查重要信息。</p>';let s=p.api.snapshotConversation('本轮提问',false);assert.equal(s.requestBound,true);assert.equal(s.done,false);assert.equal(s.texts.length,2);assert.doesNotMatch(s.texts.at(-1),/可能会出错/);p.d.querySelector('[data-talvt-turn-state]').setAttribute('data-talvt-turn-state','complete');assert.equal(p.api.snapshotConversation('本轮提问',false).done,true)}finally{p.close()}});
test('known ChatGPT page without semantic messages never returns generic footer',()=>{const p=page('https://chatgpt.com/');try{p.message('user','普通问句');p.message('assistant','ChatGPT 可能会出错。请核查重要信息。');assert.equal(p.api.snapshotConversation('普通问句',false).texts.length,0)}finally{p.close()}});
test('ChatGLM observed send is domain and composer scoped',()=>{for(const url of ['https://chatglm.cn/','https://fixture.test/']){const p=page(url);try{p.d.querySelector('form').id='search-input-box';p.d.querySelector('textarea').value='本轮提问';p.d.querySelector('form').insertAdjacentHTML('beforeend','<div class="enter-icon-container"><img class="enter_icon"></div>');let clicks=0;p.d.querySelector('.enter-icon-container').addEventListener('mousedown',e=>{assert.equal(e.button,0);clicks++});p.d.querySelector('img.enter_icon').onclick=()=>{throw Error('click only opens tooltip')};assert.equal(p.api.submit('本轮提问',0),url.includes('chatglm')?'ok':'no-send-button');assert.equal(clicks,url.includes('chatglm')?1:0)}finally{p.close()}}});
test('Gemini snapshot works when Trusted Types forbids innerHTML assignment',()=>{const p=page('https://gemini.google.com/app');try{p.w.__dsfloatRequest=34;p.api.prepareSend('A & B < C');p.d.querySelector('#messages').innerHTML='<user-query><p class="query-text-line">A &amp; B &lt; C</p></user-query><model-response><message-content><p>正确结果为四十二。</p></message-content><button aria-label="复制"></button><button aria-label="重做"></button></model-response>';const descriptor=Object.getOwnPropertyDescriptor(p.w.Element.prototype,'innerHTML');Object.defineProperty(p.w.Element.prototype,'innerHTML',{...descriptor,set(){throw new TypeError('TrustedHTML required')}});const s=p.api.snapshotConversation('A & B < C',false);assert.equal(s.ok,true);assert.equal(s.requestBound,true);assert.equal(s.done,true);assert.equal(p.api.sendEvidence('A & B < C'),'sent')}finally{p.close()}});
test('controlled rich editor never receives direct DOM overwrite when insertion fails',()=>{const p=page('https://www.kimi.com/');try{p.d.querySelector('textarea').remove();const el=p.d.createElement('div');el.setAttribute('contenteditable','true');el.setAttribute('tabindex','0');el.setAttribute('data-lexical-editor','true');Object.defineProperty(el,'isContentEditable',{value:true});el.textContent='保留的旧草稿';p.d.querySelector('form').append(el);p.d.execCommand=()=>false;assert.equal(p.api.fill('新的完整问题'),false);assert.equal(p.api.fill('新的完整问题'),false);assert.equal(el.textContent,'保留的旧草稿')}finally{p.close()}});
test('Slate placeholders do not become part of the expected question',()=>{const p=page('https://www.qianwen.com/');try{p.d.querySelector('textarea').remove();const el=p.d.createElement('div');el.setAttribute('contenteditable','true');el.setAttribute('data-slate-editor','true');Object.defineProperty(el,'isContentEditable',{value:true});el.innerHTML='<span>新问题</span><span data-slate-placeholder contenteditable="false">向千问提问</span>';p.d.querySelector('form').append(el);assert.equal(p.api.inspect().inputText,'新问题')}finally{p.close()}});
test('long baseline answer uses the same comparison bound and cannot leak into a reused question slot',()=>{const p=page('https://gemini.google.com/app');try{p.d.querySelector('#messages').innerHTML='<user-query><p class="query-text-line">旧问题</p></user-query><model-response><message-content><p></p></message-content><button aria-label="复制"></button><button aria-label="重做"></button></model-response>';p.d.querySelector('message-content p').textContent='这是旧答案'.repeat(13000);p.w.__dsfloatRequest=60;p.api.prepareSend('新问题');p.d.querySelector('.query-text-line').textContent='新问题';const s=p.api.snapshotConversation('新问题',false);assert.equal(s.texts.length,1);assert.equal(s.done,false);assert.equal(s.requestBound,true)}finally{p.close()}});
test('Slate selection and model insertion happen on separate retries, with one edit only',async()=>{const p=page('https://www.qianwen.com/');try{p.d.querySelector('textarea').remove();const el=p.d.createElement('div');el.setAttribute('contenteditable','true');el.setAttribute('tabindex','0');el.setAttribute('data-slate-editor','true');Object.defineProperty(el,'isContentEditable',{value:true});el.textContent='旧草稿';p.d.querySelector('form').append(el);let edits=0;el.addEventListener('beforeinput',e=>{edits++;assert.equal(e.cancelable,true);assert.equal(p.w.getSelection().toString(),'旧草稿');setTimeout(()=>{el.textContent=e.data},0);e.preventDefault()});assert.equal(p.api.fill('替换后的完整问题'),false);assert.equal(p.api.fill('替换后的完整问题'),false);assert.equal(edits,0);assert.equal(p.api.fill('替换后的完整问题'),false);assert.equal(p.api.fill('替换后的完整问题'),false);assert.equal(edits,1);await new Promise(r=>setTimeout(r,10));assert.equal(p.api.fill('替换后的完整问题'),true);assert.equal(el.textContent,'替换后的完整问题');assert.equal(edits,1)}finally{p.close()}});
test('Qianwen uses message identifiers and completion class, excluding navigator and input',()=>{const p=page('https://www.qianwen.com/');try{p.w.__dsfloatRequest=62;p.api.prepareSend('千问本轮问题');p.d.querySelector('#messages').innerHTML='<div data-chat-question-wrap="q1"><div class="question-text-card">千问本轮问题</div></div><div data-chat-answers-wrap="q1"><div class="answer-common-card"><div class="qk-markdown"><p>回答内容为四十二</p></div></div></div><div class="rn-right-navigator">上一轮问题导航</div>';let s=p.api.snapshotConversation('千问本轮问题',false);assert.equal(s.requestBound,true);assert.equal(s.texts.length,2);assert.equal(s.done,false);assert.match(s.texts.at(-1),/四十二/);p.d.querySelector('.qk-markdown').classList.add('qk-markdown-complete');assert.equal(p.api.snapshotConversation('千问本轮问题',false).done,true)}finally{p.close()}});

test('Lexical replaces previous draft through staged beforeinput exactly once',async()=>{const p=page('https://www.kimi.com/');try{p.d.querySelector('textarea').remove();const el=p.d.createElement('div');el.setAttribute('contenteditable','true');el.setAttribute('tabindex','0');el.setAttribute('data-lexical-editor','true');Object.defineProperty(el,'isContentEditable',{value:true});el.textContent='old draft';p.d.querySelector('form').append(el);let edits=0;p.d.execCommand=()=>{throw Error('must not bypass editor model')};el.addEventListener('beforeinput',e=>{edits++;assert.equal(p.w.getSelection().toString(),'old draft');setTimeout(()=>{el.textContent=e.data},0);e.preventDefault()});assert.equal(p.api.fill('complete new question'),false);assert.equal(p.api.fill('complete new question'),false);assert.equal(p.api.fill('complete new question'),false);await new Promise(r=>setTimeout(r,10));assert.equal(p.api.fill('complete new question'),true);assert.equal(edits,1)}finally{p.close()}});

test('Kimi reads only final answer and requires answer actions for completion',()=>{const p=page('https://www.kimi.com/');try{p.w.__dsfloatRequest=70;p.api.prepareSend('Kimi current question');p.d.querySelector('#messages').innerHTML='<div class="segment segment-user"><span class="user-content__text">Kimi current question</span></div><div class="segment segment-assistant"><div class="toolcall-flow"><div class="markdown-container"><p>private reasoning must stay separate</p></div></div><div class="markdown-container"><div class="markdown"><p>final answer forty two</p></div></div></div>';let s=p.api.snapshotConversation('Kimi current question',false);assert.equal(s.requestBound,true);assert.equal(s.texts.length,2);assert.match(s.texts.at(-1),/final answer/);assert.doesNotMatch(s.texts.at(-1),/reasoning/);assert.equal(s.done,false);p.d.querySelector('.segment-assistant').insertAdjacentHTML('beforeend','<div class="segment-assistant-actions"><svg name="Copy"></svg><svg name="Refresh"></svg></div>');assert.equal(p.api.snapshotConversation('Kimi current question',false).done,true)}finally{p.close()}});

test('ChatGLM bounds questions and final answers to each conversation item',()=>{const p=page('https://chatglm.cn/');try{p.w.__dsfloatRequest=72;p.api.prepareSend('GLM current question');p.d.querySelector('#messages').innerHTML='<div class="history-item">GLM current question</div><div class="conversation-item"><div class="question"><div class="question-txt">GLM current question</div></div><div class="answer"><div class="thinking-content"><div class="answer-content-wrap"><p>reasoning excluded</p></div></div><div class="answer-content-wrap"><p>final answer GLM 42</p></div></div></div>';let s=p.api.snapshotConversation('GLM current question',false);assert.equal(s.requestBound,true);assert.equal(s.texts.length,2);assert.match(s.texts.at(-1),/final answer GLM/);assert.doesNotMatch(s.texts.at(-1),/reasoning/);assert.equal(s.done,false);p.d.querySelector('.answer').insertAdjacentHTML('beforeend','<i class="shim copy canuse"></i><i class="shim regenerate canuse"></i>');assert.equal(p.api.snapshotConversation('GLM current question',false).done,true)}finally{p.close()}});
