param([ValidateSet('panel','shot','restore')][string]$Window='panel',[switch]$MoveCursor,[int]$X=0,[int]$Y=0)
$ErrorActionPreference='Stop'
Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class FixtureWindowProbe {
public delegate bool Callback(IntPtr h,IntPtr p);
[DllImport("user32.dll")]static extern bool EnumWindows(Callback cb,IntPtr p);
[DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
[DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetWindowTextW(IntPtr h,StringBuilder b,int count);
[DllImport("user32.dll")]static extern bool GetClientRect(IntPtr h,out Rect r);
[DllImport("user32.dll")]static extern bool ClientToScreen(IntPtr h,ref Point p);
[DllImport("user32.dll")]static extern uint GetDpiForWindow(IntPtr h);
[DllImport("user32.dll")]static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);
[DllImport("user32.dll")]static extern bool GetCursorPos(out Point p);
[DllImport("user32.dll")]static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")]static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int t,uint flags);
struct Point{public int x,y;}struct Rect{public int left,top,right,bottom;}
public static string Info(uint target,string title,bool move){var old=SetThreadDpiAwarenessContext(new IntPtr(-4));try{IntPtr found=IntPtr.Zero;EnumWindows((h,p)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid==target){var b=new StringBuilder(256);GetWindowTextW(h,b,256);if(b.ToString()==title)found=h;}return true;},IntPtr.Zero);if(found==IntPtr.Zero)throw new InvalidOperationException("Fixture window missing");Rect r;GetClientRect(found,out r);var origin=new Point();ClientToScreen(found,ref origin);Point cursor;GetCursorPos(out cursor);if(move){SetWindowPos(found,new IntPtr(-1),0,0,0,0,0x13);SetCursorPos(origin.x+60,origin.y+80);}return "{\"x\":"+origin.x+",\"y\":"+origin.y+",\"width\":"+r.right+",\"height\":"+r.bottom+",\"dpi\":"+GetDpiForWindow(found)+",\"cursorX\":"+cursor.x+",\"cursorY\":"+cursor.y+"}";}finally{SetThreadDpiAwarenessContext(old);}}
public static void Restore(int x,int y){var old=SetThreadDpiAwarenessContext(new IntPtr(-4));try{SetCursorPos(x,y);}finally{SetThreadDpiAwarenessContext(old);}}
}
"@
if($Window -eq 'restore'){[FixtureWindowProbe]::Restore($X,$Y);return}
$app=Get-Process deepseek-float
if(@($app).Count -ne 1){throw 'Expected one isolated test instance'}
$title=if($Window -eq 'panel'){'DeepSeek 悬浮面板'}else{'DeepSeek 截图遮罩'}
[FixtureWindowProbe]::Info($app.Id,$title,$MoveCursor.IsPresent)
