$ErrorActionPreference = 'Stop'

$launcher = Join-Path $PSScriptRoot 'run-beta2-native-acceptance.ps1'
if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
    throw "Native launcher is missing: $launcher"
}
$tokens = $null
$parseErrors = $null
[Management.Automation.Language.Parser]::ParseFile($launcher, [ref]$tokens, [ref]$parseErrors) | Out-Null
if ($parseErrors.Count -ne 0) {
    throw "Native launcher has PowerShell parse errors: $($parseErrors | Out-String)"
}
$launcherSource = Get-Content -LiteralPath $launcher -Raw
$launcherAst = [Management.Automation.Language.Parser]::ParseFile($launcher, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count -ne 0) {
    throw "Native launcher has PowerShell parse errors: $($parseErrors | Out-String)"
}
$planMutationFunction = $launcherAst.Find({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
    $node.Name -eq 'Set-Beta2PlanLaunchMetadata'
}, $true)
if (-not $planMutationFunction) {
    throw 'Launch-plan metadata helper is missing.'
}
Invoke-Expression $planMutationFunction.Extent.Text
$uiReadyFunction = $launcherAst.Find({
    param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
    $node.Name -eq 'Test-Beta2UiReadyLine'
}, $true)
if (-not $uiReadyFunction) { throw 'UI Ready log matcher is missing.' }
Invoke-Expression $uiReadyFunction.Extent.Text
if (-not (Test-Beta2UiReadyLine '[ball-ready] 收到 ball_ready → 准备主线程启动显形判定') -or
    (Test-Beta2UiReadyLine '[ball-ready] 等待 ball_ready 超时（~3s 兜底）→ 准备显形判定')) {
    throw 'UI Ready matcher accepts a timeout or rejects the actual handshake.'
}
$minimalPlan = '{}' | ConvertFrom-Json
$testStartupId = [Guid]::NewGuid().ToString('N')
$testStartedAt = [DateTime]::UtcNow.ToString('o')
Set-Beta2PlanLaunchMetadata -Plan $minimalPlan -StartupId $testStartupId -StartedAtUtc $testStartedAt
if ($minimalPlan.launches_so_far -ne 1 -or $minimalPlan.startup_id -ne $testStartupId -or
    $minimalPlan.started_at_utc -ne $testStartedAt) {
    throw 'Launch-plan metadata was not added to a JSON object that lacked optional fields.'
}
$observerPattern = '(?s)\$nativeObserver\s*=\s*@''\r?\n(.*?)\r?\n''@'
$observerMatch = [regex]::Match($launcherSource, $observerPattern)
if (-not $observerMatch.Success) { throw 'Win32 observer source is missing.' }
if (-not $observerMatch.Groups[1].Value.Contains('IsWindow(')) {
    throw 'Win32 observer does not verify HWND validity.'
}
Add-Type -TypeDefinition $observerMatch.Groups[1].Value -ErrorAction Stop
if ($launcherSource.Contains('Get-CimInstance Win32_Process')) {
    throw 'Native monitor still depends on the unavailable WMI process provider.'
}

$childInfo = [Diagnostics.ProcessStartInfo]::new()
$childInfo.FileName = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$childInfo.UseShellExecute = $false
$childInfo.ArgumentList.Add('-NoLogo')
$childInfo.ArgumentList.Add('-NoProfile')
$childInfo.ArgumentList.Add('-Command')
$childInfo.ArgumentList.Add('Start-Sleep -Seconds 3')
$childProcess = [Diagnostics.Process]::Start($childInfo)
if (-not $childProcess) { throw 'Could not start the short-lived observer fixture process.' }
try {
    $processDeadline = [DateTime]::UtcNow.AddSeconds(2)
    $observedChild = $false
    do {
        $observedChild = @([Beta2Win32Windows]::DescendantsOf([uint32]$PID) |
            Where-Object { $_.ProcessId -eq $childProcess.Id -and $_.ParentProcessId -eq $PID }).Count -eq 1
        if ($observedChild) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $processDeadline)
    if (-not $observedChild) { throw 'Win32 process-tree observer did not associate the child PID with its parent.' }
} finally {
    if (-not $childProcess.WaitForExit(5000)) {
        throw 'Observer fixture child did not exit naturally.'
    }
    $childProcess.Dispose()
}

$testRoot = Join-Path $env:TEMP ("beta2-launcher-test-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null

$exe = Join-Path $testRoot 'fake-candidate.exe'
$pdb = Join-Path $testRoot 'fake-candidate.pdb'
$identityPath = Join-Path $testRoot 'candidate-identity.json'
$planPath = Join-Path $testRoot 'native-root-plan.json'
[IO.File]::WriteAllText($exe, 'not an executable; preflight must never start it')
[IO.File]::WriteAllText($pdb, 'diagnostic fixture')

$exeHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
$pdbHash = (Get-FileHash -LiteralPath $pdb -Algorithm SHA256).Hash
$identity = [ordered]@{
    project = @{ git_head = 'test-head' }
    candidate = @{
        executable = $exe
        exe_sha256 = $exeHash
        pdb = $pdb
        pdb_sha256 = $pdbHash
        pdb_guid = '00000000-0000-0000-0000-000000000001'
        pdb_age = 1
        pdb_id_matches_exe_codeview = $true
    }
    isolation = @{ identifier = 'com.deepseekfloat.beta2.isolated.launchertest' }
}
$identityJson = $identity | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText($identityPath, $identityJson)
$identityHash = (Get-FileHash -LiteralPath $identityPath -Algorithm SHA256).Hash
$dataRoot = Join-Path $testRoot 'native-data'
$plan = [ordered]@{
    candidate_identity_path = $identityPath
    candidate_identity_sha256 = $identityHash
    executable = $exe
    exe_sha256 = $exeHash
    pdb = $pdb
    pdb_sha256 = $pdbHash
    pdb_guid_age = '00000000-0000-0000-0000-000000000001/1'
    app_identifier = 'com.deepseekfloat.beta2.isolated.launchertest'
    working_directory = $testRoot
    arguments = @('--beta2-isolated-data-root', $dataRoot)
    isolation_root = $dataRoot
    evidence_root = $testRoot
    launch_authorized = $false
    launches_so_far = 0
    max_launches = 1
    environment_policy = @{
        remove_all_inherited_webview2_variables = $true
        set_webview2_variables = $false
        set_temp_or_tmp = $false
    webview2_data_directory_source = 'application internal isolation paths'
    }
    diagnostic_environment = @{
        variable = 'DSFLOAT_DIAG'
        value = '1'
        expected_level = 'Meta'
    }
    diagnostic_log = Join-Path $dataRoot 'diagnostics\d1-debug.log'
}
[IO.File]::WriteAllText($planPath, ($plan | ConvertTo-Json -Depth 8))

$powershell = (Get-Command pwsh -ErrorAction Stop).Source
$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $powershell
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
foreach ($argument in @('-NoProfile', '-File', $launcher, '-PlanPath', $planPath, '-PreflightOnly')) {
    $startInfo.ArgumentList.Add($argument)
}
$startInfo.EnvironmentVariables['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $testRoot 'forbidden-profile'
$startInfo.EnvironmentVariables['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = '--remote-debugging-port=0'
$process = [Diagnostics.Process]::Start($startInfo)
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
$process.WaitForExit()

if ($process.ExitCode -ne 0) {
    throw "Launcher preflight failed ($($process.ExitCode)): $($stderr.Result)"
}
$result = $stdout.Result | ConvertFrom-Json
if (-not $result.preflight_passed -or $result.candidate_started) {
    throw "Preflight did not remain a no-launch check: $($stdout.Result)"
}
if (@($result.child_webview2_variables).Count -ne 0) {
    throw "WEBVIEW2 variables would reach the candidate: $($result.child_webview2_variables -join ', ')"
}
if (@($result.removed_inherited_webview2_variables).Count -ne 2 -or
    @($result.removed_inherited_webview2_variables | Where-Object { -not $_.StartsWith('WEBVIEW2_', [StringComparison]::OrdinalIgnoreCase) }).Count -ne 0) {
    throw "Launcher removed variables outside WEBVIEW2_*: $($result.removed_inherited_webview2_variables -join ', ')"
}
if (-not $result.child_system_root_present -or -not $result.child_path_present) {
    throw 'Launcher removed ordinary Windows process environment variables.'
}
if ($result.child_diagnostic_value -ne '1' -or $result.diagnostic_level -ne 'Meta' -or
    $result.diagnostic_log -ine (Join-Path $dataRoot 'diagnostics\d1-debug.log') -or
    -not $result.diagnostic_log_within_isolation_root) {
    throw "Diagnostic environment or log target is not isolated: $($stdout.Result)"
}
if ($result.arguments.Count -ne 2 -or $result.arguments[0] -ne '--beta2-isolated-data-root' -or $result.arguments[1] -ne $dataRoot) {
    throw "Isolation arguments were changed: $($result.arguments | ConvertTo-Json -Compress)"
}
if (Test-Path -LiteralPath $dataRoot) {
    throw 'Preflight created the isolation root.'
}

Write-Output 'PASS: WEBVIEW2_* values are removed; DSFLOAT_DIAG=1 targets the isolated Meta log; isolation arguments remain; candidate was not started.'
