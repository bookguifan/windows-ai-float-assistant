import test from 'node:test';
import assert from 'node:assert/strict';
import {scoreTranscript} from '../scripts/lib/ocr-score.mjs';
test('OCR score counts Unicode characters and genuine recognition errors',()=>{
 assert.equal(scoreTranscript('中文A42','中文A42').exact,true);
 assert.equal(scoreTranscript('中文A42','中问A42').characterErrorRate,0.2);
 assert.equal(scoreTranscript('O0 I1 lL','00 I1 lL').edits,1);
 assert.equal(scoreTranscript('42','答案是42').exact,false);
});
test('OCR whitespace accuracy is measured separately from character accuracy',()=>{
 const s=scoreTranscript('第一行\n第二行','第一行 第二行');
 assert.equal(s.exact,true);
 assert.equal(s.linesPreserved,false);
 assert.equal(scoreTranscript('第一行\n第二行','```text\n第一行\n第二行\n```').linesPreserved,true);
});
test('table layout normalization accepts Markdown without hiding changed values',()=>{
 const expected='项目 数量\n铅笔 12';
 assert.equal(scoreTranscript(expected,'| 项目 | 数量 |\n| --- | --- |\n| 铅笔 | 12 |','table').exact,true);
 assert.equal(scoreTranscript(expected,'| 项目 | 数量 |\n| --- | --- |\n| 铅笔 | 13 |','table').edits,1);
 assert.equal(scoreTranscript('价格：2.50','价格:2.50').edits,1);
});
