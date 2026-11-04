import {test,expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
const html = readFileSync(new URL('../poc/settings.html',import.meta.url),'utf8');

test('copy selector and both screenshot hotkey recorders fit a narrow settings window',async({page})=>{
  await page.setViewportSize({width:460,height:650});
  await page.route('**/*', route=>route.abort());
  await page.setContent(html.replace('<head>',`<head><script>window.__TAURI_INTERNALS__={invoke:(c,a)=>Promise.resolve(c==='config_get'?{hotkey:'Ctrl+Alt+2',privileged_capture:true}:c==='hotkey_status'?'Ctrl+Alt+2':c==='ocr_hotkey_status'?null:c==='safe_copy_status'?{smart_enabled:true,available:true,enabled:true,shortcut:'ctrl-c',mode:'compatibility',session_only:true}:true)};</script>`));
  await page.evaluate(()=>__dsfSettingsInit({prompts:[]}));
  const select=page.getByRole('combobox',{name:'复制快捷键'});
  await expect(select).toBeVisible();
  await expect(select).toHaveValue('ctrl-c');
  await expect(page.getByRole('textbox',{name:'截图提问快捷键'})).toHaveValue('Ctrl+Alt+2');
  await expect(page.getByRole('textbox',{name:'截图识字快捷键'})).toHaveValue('');
  for(const width of [460,360]){
    await page.setViewportSize({width,height:650});
    for(const control of [select,page.locator('#hotkey-input'),page.locator('#ocr-hotkey-input')]){
      const box=await control.boundingBox();
      expect(box.height).toBeGreaterThanOrEqual(34);
      expect(box.x+box.width).toBeLessThanOrEqual(width);
    }
    // The existing settings page supports a 460px window; at 360px only these
    // new rows are under test, since other existing sections are wider.
    if(width===460) expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  }
  await page.setViewportSize({width:560,height:750});
  await select.scrollIntoViewIfNeeded();
  await page.screenshot({path:'.test-artifacts/settings-ocr-hotkeys.png',fullPage:true});
});
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
