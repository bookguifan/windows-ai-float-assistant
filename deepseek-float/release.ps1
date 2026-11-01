<#
.SYNOPSIS
  DeepSeek 悬浮助手 · 调用统一 Node 发布构建入口。

.DESCRIPTION
  发布依赖、版本、前端、Rust、安全策略、Release 构建与哈希逻辑均由
  scripts/build-release.mjs 执行。本脚本只保留 PowerShell 兼容入口。

.PARAMETER SkipFrontBuild
  将 --no-frontend 转交给统一 Node 构建入口。

.PARAMETER UseSccache
  使用已安装的 sccache；默认关闭。

.PARAMETER NodePath
  显式选择已安装的 node.exe。其版本和 pnpm 子进程版本仍由统一入口校验。

.PARAMETER CheckToolchainOnly
  只校验 Node、pnpm 版本及 pnpm 所用 Node，不运行构建。

.EXAMPLE
  ./release.ps1
  ./release.ps1 -SkipFrontBuild
  ./release.ps1 -UseSccache
  ./release.ps1 -NodePath 'C:\Tools\node\node.exe' -CheckToolchainOnly
#>
[CmdletBinding()]
param(
  [switch]$SkipFrontBuild,
  [switch]$UseSccache,
  [string]$NodePath,
  [switch]$CheckToolchainOnly
)

$ErrorActionPreference = 'Stop'
$Project = Split-Path -Parent $MyInvocation.MyCommand.Path

if ($NodePath) {
  if (-not (Test-Path -LiteralPath $NodePath -PathType Leaf)) {
    throw "NodePath does not name an existing file: $NodePath"
  }
  $NodeExecutable = (Resolve-Path -LiteralPath $NodePath).Path
} else {
  $NodeExecutable = (Get-Command node -CommandType Application -ErrorAction Stop).Source
}

if ($UseSccache) {
  $env:RUSTC_WRAPPER = 'sccache'
}

$Arguments = @('scripts/build-release.mjs')
if ($SkipFrontBuild) { $Arguments += '--no-frontend' }
if ($CheckToolchainOnly) { $Arguments += '--check-toolchain-only' }

$ExitCode = 0
Push-Location $Project
try {
  & $NodeExecutable @Arguments
  if ($null -ne $LASTEXITCODE) { $ExitCode = $LASTEXITCODE }
} finally {
  Pop-Location
}
exit $ExitCode
