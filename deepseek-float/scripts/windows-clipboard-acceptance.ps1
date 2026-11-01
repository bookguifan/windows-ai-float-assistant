[CmdletBinding()]
param(
  [ValidateSet('Plan', 'ListFormats', 'VerifyText', 'VerifyFormats')]
  [string]$Action = 'Plan',
  [string]$ExpectedText,
  [string[]]$ExpectedFormat = @(),
  [switch]$AcknowledgeClipboardRead
)

$ErrorActionPreference = 'Stop'

if ($Action -eq 'Plan') {
  @(
    'This script never writes to the clipboard.'
    'Plan mode does not read the clipboard.'
    'Read actions require -AcknowledgeClipboardRead and an STA PowerShell session.'
    'Use only a dedicated test session containing known, disposable test data.'
  ) | Write-Output
  return
}

if (-not $AcknowledgeClipboardRead) {
  throw 'Clipboard reads require -AcknowledgeClipboardRead.'
}
if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne 'STA') {
  throw 'Run this read-only verifier in an STA session (for example, powershell.exe -NoProfile -STA).'
}

Add-Type -AssemblyName System.Windows.Forms
$data = [Windows.Forms.Clipboard]::GetDataObject()
if ($null -eq $data) {
  throw 'The clipboard returned no data object.'
}

if ($Action -eq 'ListFormats') {
  $data.GetFormats($false) | Sort-Object -Unique | Write-Output
  return
}

if ($Action -eq 'VerifyFormats') {
  if ($ExpectedFormat.Count -eq 0) {
    throw 'Provide one or more -ExpectedFormat values.'
  }
  $formats = @($data.GetFormats($false))
  $missing = @($ExpectedFormat | Where-Object { $_ -notin $formats })
  if ($missing.Count -gt 0) {
    throw ('Missing clipboard formats: ' + ($missing -join ', '))
  }
  Write-Output ('PASS: all expected formats remain: ' + ($ExpectedFormat -join ', '))
  return
}

if ($null -eq $ExpectedText) {
  throw 'Provide -ExpectedText for VerifyText.'
}
$actualText = [Windows.Forms.Clipboard]::GetText([Windows.Forms.TextDataFormat]::UnicodeText)
$encoding = [Text.Encoding]::UTF8
$sha = [Security.Cryptography.SHA256]::Create()
try {
  $expectedHash = [BitConverter]::ToString($sha.ComputeHash($encoding.GetBytes($ExpectedText))).Replace('-', '')
  $actualHash = [BitConverter]::ToString($sha.ComputeHash($encoding.GetBytes($actualText))).Replace('-', '')
} finally {
  $sha.Dispose()
}
if (-not [string]::Equals($actualText, $ExpectedText, [StringComparison]::Ordinal)) {
  throw "Clipboard text mismatch. Expected SHA256=$expectedHash; actual SHA256=$actualHash."
}
Write-Output "PASS: exact Unicode text matched; SHA256=$actualHash"
