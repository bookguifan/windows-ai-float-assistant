import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';
function setup() {
 const html=readFileSync(new URL('../poc/ball.html',import.meta.url),'utf8');
 const dom=new JSDOM(html,{runScripts:'outside-only',pretendToBeVisual:true});
 const calls=[];dom.window.__TAURI_INTERNALS__={invoke:(command,args)=>{calls.push({command,args});return Promise.resolve()}};
 for(const script of html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi))dom.window.eval(script[1]);
 const ball=dom.window.document.getElementById('ball');
 const mouse=(type,x=32,y=32,button=0)=>ball.dispatchEvent(new dom.window.MouseEvent(type,{bubbles:true,cancelable:true,clientX:x,clientY:y,button}));
 return {dom,calls,mouse,count:command=>calls.filter(c=>c.command===command).length};
}
test('left click opens panel once and rapid double click is debounced',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mouseup');p.mouse('mousedown');p.mouse('mouseup');assert.equal(p.count('toggle_panel'),1)}finally{p.dom.window.close()}});
test('drag beyond threshold starts native drag once and never toggles panel',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mousemove',40,40);p.mouse('mousemove',50,50);p.mouse('mouseup',50,50);assert.equal(p.count('plugin:window|start_dragging'),1);assert.equal(p.count('toggle_panel'),0)}finally{p.dom.window.close()}});
test('small movement remains a click',()=>{const p=setup();try{p.mouse('mousedown');p.mouse('mousemove',34,34);p.mouse('mouseup',34,34);assert.equal(p.count('toggle_panel'),1);assert.equal(p.count('plugin:window|start_dragging'),0)}finally{p.dom.window.close()}});
test('blur cancels unfinished click',()=>{const p=setup();try{p.mouse('mousedown');p.dom.window.dispatchEvent(new p.dom.window.Event('blur'));p.mouse('mouseup');assert.equal(p.count('toggle_panel'),0)}finally{p.dom.window.close()}});
test('right click only requests the context menu',()=>{const p=setup();try{p.mouse('mousedown',32,32,2);p.mouse('mouseup',32,32,2);p.mouse('contextmenu',32,32,2);assert.equal(p.count('ball_contextmenu'),1);assert.equal(p.count('toggle_panel'),0);assert.equal(p.count('plugin:window|start_dragging'),0)}finally{p.dom.window.close()}});
