<#
.SYNOPSIS
  DeepSeek 悬浮助手 · 一键产出合规绿色版交付物
  `deepseek-float/release/deepseek-float.exe` + 同目录 HASH.txt（SHA256）。

.DESCRIPTION
  把 README「发布前 checklist」收敛成单一幂等入口，按序执行：
  前端构建 → 前端门禁(pnpm check) → fmt --check → clippy -D warnings → test --lib
  → release 构建(custom-protocol, CARGO_INCREMENTAL=0) → 复制交付位 → 校验哈希。
  任一步失败即打印红叉并以非 0 退出，绝不静默跳过。

.PARAMETER SkipFrontBuild
  已确认 dist 为最新时跳过 pnpm build（仍需 pnpm check）。默认开启前端构建。

.PARAMETER UseSccache
  使用 sccache 缓存加速后续 release 增量编译（需已安装 sccache，未装则报错）。
  默认关闭，避免强制安装系统工具。

.EXAMPLE
  ./release.ps1
  ./release.ps1 -SkipFrontBuild
  ./release.ps1 -UseSccache
#>
[CmdletBinding()]
param(
  [switch]$SkipFrontBuild,
  [switch]$UseSccache
)

$ErrorActionPreference = 'Stop'
$Project   = Split-Path -Parent $MyInvocation.MyCommand.Path  # deepseek-float 根
$Manifest  = Join-Path $Project 'src-tauri\Cargo.toml'
$Exe       = Join-Path $Project 'src-tauri\target\release\deepseek-float.exe'
$Deliver   = Join-Path $Project 'release\deepseek-float.exe'
$HashFile  = Join-Path $Project 'release\HASH.txt'

function Step($n, $msg) { Write-Host "`n[$n/8] $msg" -ForegroundColor Cyan }
function Fail($msg)     { Write-Host "✗ $msg" -ForegroundColor Red; exit 1 }

Set-Location $Project

Step 1 '前端依赖 + 构建 (pnpm install && pnpm build)'
if (-not $SkipFrontBuild) {
  pnpm install; if ($LASTEXITCODE -ne 0) { Fail 'pnpm install 失败' }
  pnpm build;   if ($LASTEXITCODE -ne 0) { Fail 'pnpm build 失败' }
}
npm run check; if ($LASTEXITCODE -ne 0) { Fail 'pnpm check（CSP/内联JS/版本）失败' }

Step 2 'Rust 格式 (cargo fmt --check)'
cargo fmt --manifest-path $Manifest --check
if ($LASTEXITCODE -ne 0) {
  Fail "代码未格式化。请先执行: cargo fmt --manifest-path `"$Manifest`""
}

Step 3 'Rust lint (clippy --all-targets -- -D warnings)'
cargo clippy --manifest-path $Manifest --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Fail 'clippy 不通过（-D warnings）' }

Step 4 'Rust 单测 (cargo test --lib)'
cargo test --manifest-path $Manifest --lib
if ($LASTEXITCODE -ne 0) { Fail '单元测试失败' }

Step 5 'release 构建 (custom-protocol · CARGO_INCREMENTAL=0)'
# ★ 规避 README L267 的坑：受限终端里 LTO 写临时目录会「compiler unexpectedly panicked
#   (os error 5)」。release 本就不需要增量，这里强制关闭以稳定构建。
$env:CARGO_INCREMENTAL = '0'
if ($UseSccache) {
  $env:RUSTC_WRAPPER = 'sccache'
}
cargo build --locked --release --features custom-protocol --manifest-path $Manifest
if ($LASTEXITCODE -ne 0) {
  Fail 'release 构建失败。若为编译器 panic（os error 5 / 写临时目录被拒），请结束占用后在不受限环境重试。'
}

Step 6 '交付到 release/'
if (-not (Test-Path $Exe)) { Fail '未找到 release exe，构建产物缺失' }
New-Item -ItemType Directory -Force -Path (Split-Path $Deliver) | Out-Null
Copy-Item $Exe $Deliver -Force

Step 7 '校验 (FullName / Length / LastWriteTime / SHA256)'
$i = Get-Item $Deliver
$sha = (Get-FileHash $Deliver -Algorithm SHA256).Hash
Write-Host ("`n  FullName  : {0}`n  Length    : {1}`n  LastWrite : {2}`n  SHA256    : {3}" -f `
  $i.FullName, $i.Length, $i.LastWriteTime, $sha) -ForegroundColor Green
$sha | Set-Content -Path $HashFile

Step 8 '完成'
Write-Host "✓ 交付物就绪：$Deliver`n  哈希已写：$HashFile`n  （分发给用户只需拷贝这个 exe）" -ForegroundColor Green
