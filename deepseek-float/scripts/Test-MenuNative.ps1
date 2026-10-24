# Windows 真机交互检查：仅操作 deepseek-float.exe 的悬浮球，需当前实例开启诊断日志。
$ErrorActionPreference='Stop'
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class FloatInteraction {
public delegate bool Callback(IntPtr h,IntPtr p);
[DllImport("user32.dll")] public static extern bool EnumWindows(Callback f,IntPtr p);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
[DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
[DllImport("user32.dll")] public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
[DllImport("user32.dll")] public static extern bool GetCursorPos(out Point p);
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr z,int x,int y,int w,int t,uint f);
[DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
public struct Rect {public int l,t,r,b;} public struct Point {public int x,y;}
public static IntPtr Find(uint target,string match,bool cls=false){IntPtr found=IntPtr.Zero;EnumWindows((h,p)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid==target){var s=new StringBuilder(256);if(cls)GetClassName(h,s,256);else GetWindowText(h,s,256);if(s.ToString()==match)found=h;}return true;},IntPtr.Zero);return found;}
}
"@
$appProcess=Get-Process deepseek-float
$ball=[FloatInteraction]::Find($appProcess.Id,'DeepSeek 悬浮助手')
if($ball -eq 0){throw 'Required native windows missing'}
$diagPath=Join-Path $env:LOCALAPPDATA 'com.deepseekfloat.desktop/d1-debug.log'
if(-not (Test-Path $diagPath)){throw 'Diagnostic log required for capture assertion'}
$diagStart=(Get-Item $diagPath).Length
$before=New-Object FloatInteraction+Rect
$cursor=New-Object FloatInteraction+Point
[FloatInteraction]::GetWindowRect($ball,[ref]$before)|Out-Null
[FloatInteraction]::GetCursorPos([ref]$cursor)|Out-Null
try {
 [FloatInteraction]::SetCursorPos([int](($before.l+$before.r)/2),[int](($before.t+$before.b)/2))|Out-Null
 [FloatInteraction]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 80
 [FloatInteraction]::SetCursorPos([int](($before.l+$before.r)/2)+25,[int](($before.t+$before.b)/2)+25)|Out-Null
 Start-Sleep -Milliseconds 80
 [FloatInteraction]::SetCursorPos([int](($before.l+$before.r)/2)+45,[int](($before.t+$before.b)/2)+45)|Out-Null
 Start-Sleep -Milliseconds 100
 [FloatInteraction]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 150
 $after=New-Object FloatInteraction+Rect
 [FloatInteraction]::GetWindowRect($ball,[ref]$after)|Out-Null
 if($after.l -eq $before.l -and $after.t -eq $before.t){throw 'Native ball drag did not move window'}
 'PASS native ball drag moves window'
} finally {
 [FloatInteraction]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 [FloatInteraction]::SetWindowPos($ball,[IntPtr]::Zero,$before.l,$before.t,0,0,0x15)|Out-Null
 [FloatInteraction]::SetCursorPos($cursor.x,$cursor.y)|Out-Null
}
Start-Sleep -Milliseconds 900
if(Test-Path $diagPath){
 $stream=[IO.File]::Open($diagPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
 try{$stream.Seek($diagStart,[IO.SeekOrigin]::Begin)|Out-Null;$reader=[IO.StreamReader]::new($stream);$newLines=$reader.ReadToEnd()}finally{$stream.Dispose()}
 if($newLines -match 'trigger_selection 进入'){throw 'Dragging ball incorrectly triggered global text capture'}
}
'PASS native ball drag does not trigger global text capture'