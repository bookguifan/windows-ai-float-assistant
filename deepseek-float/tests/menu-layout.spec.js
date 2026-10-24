import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
const html=readFileSync(new URL('../poc/menu.html',import.meta.url),'utf8');
async function setup(page){
 await page.setViewportSize({width:200,height:200});
 await page.route('**/*',route=>route.abort());
 await page.setContent(html.replace('<head>','<head><script>window.__TAURI_INTERNALS__={invoke:(cmd)=>cmd=== "context_menu_switch_site" ? Promise.reject("切换失败："+"详细错误".repeat(40)) : Promise.resolve()};</script>'));
 await page.evaluate(()=>window.__dsfMenuSet({generation:1,ballVisible:true,panelVisible:false,sites:Array.from({length:6},(_,i)=>({label:'站点'+i,url:'https://example.test/'+i})),panelUrl:'https://example.test/0'}));
}
test('compact main actions fit 200x200',async({page})=>{await setup(page);const boxes=await page.locator('#mainView button').evaluateAll(nodes=>nodes.map(n=>{const b=n.getBoundingClientRect();return {top:b.top,bottom:b.bottom,right:b.right}}));expect(boxes).toHaveLength(9);expect(boxes.every(b=>b.top>=0&&b.bottom<=200&&b.right<=200)).toBeTruthy()});
test('site choices and long errors fit without moving actions outside window',async({page})=>{await setup(page);await page.locator('#switchSite').click();await page.locator('[data-site]').first().click();await expect(page.locator('#siteError')).toContainText('切换失败');const bounds=await page.locator('main').boundingBox();expect(bounds.y+bounds.height).toBeLessThanOrEqual(200);await expect(page.locator('#siteBack')).toBeVisible();await page.keyboard.press('Escape');await expect(page.locator('#mainView')).toBeVisible()});

test('all twenty custom sites remain accessible in the compact menu',async({page})=>{
 await setup(page);
 await page.evaluate(()=>__dsfMenuSet({generation:2,sites:Array.from({length:25},(_,i)=>({label:'站点'+i,url:'https://example.test/'+i})),panelUrl:'https://example.test/24'}));
 await page.locator('#switchSite').click();await page.keyboard.press('End');
 await expect(page.locator('[data-site]').last()).toBeFocused();
 const last=await page.locator('[data-site]').last().boundingBox();expect(last.y).toBeGreaterThanOrEqual(0);expect(last.y+last.height).toBeLessThanOrEqual(200);
 expect(await page.locator('#siteList').evaluate(el=>el.scrollHeight>el.clientHeight)).toBeTruthy();
 await page.screenshot({path:'.test-artifacts/first-batch/menu-sites.png'});
});
