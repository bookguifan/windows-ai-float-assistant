import {test,expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
const html = readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8');
test('settings save errors and custom site controls fit the existing narrow window',async({page})=>{
  await page.setViewportSize({width:460,height:560});
  await page.route('**/*', route=>route.abort());
  await page.setContent(html.replace('<head>','<head><script>window.__TAURI_INTERNALS__={invoke:(c,a)=>c==="settings_save"?Promise.reject("写入失败："+"详细原因".repeat(50)):Promise.resolve(c==="config_get"?{custom_sites:[{label:"很长的自定义站点名称",url:"https://example.test/"+"long-path/".repeat(30)}]}:true)};</script>'));
  await page.evaluate(()=>__dsfSettingsInit({prompts:[{id:'explain',label:'解释',instruction:'解释',is_builtin:true}]}));
  await page.locator('[data-act="hide"]').click();
  await expect(page.locator('#prompt-save-status')).toContainText('保存失败');
  await expect(page.locator('#prompt-save-retry')).toBeVisible();
  await expect(page.locator('#site-save')).toBeVisible();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(460);
  await page.screenshot({path:'.test-artifacts/first-batch/settings.png',fullPage:true});
});
