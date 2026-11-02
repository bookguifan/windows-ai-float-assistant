[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$PlanPath,
    [switch]$PreflightOnly
)

$ErrorActionPreference = 'Stop'

function Get-Sha256([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToUpperInvariant()
}

function Get-FileSnapshot([string[]]$Paths) {
    @($Paths | ForEach-Object {
        if (Test-Path -LiteralPath $_ -PathType Leaf) {
            $item = Get-Item -LiteralPath $_
            [ordered]@{ path = $_; exists = $true; bytes = $item.Length; sha256 = Get-Sha256 $_ }
        } else {
            [ordered]@{ path = $_; exists = $false; bytes = $null; sha256 = $null }
        }
    })
}

function Get-RunValue([string]$Path, [string]$Name) {
    try {
        (Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
    } catch {
        $null
    }
}

function Set-Beta2PlanLaunchMetadata($Plan, [string]$StartupId, [string]$StartedAtUtc) {
    Add-Member -InputObject $Plan -MemberType NoteProperty -Name launches_so_far -Value 1 -Force
    Add-Member -InputObject $Plan -MemberType NoteProperty -Name startup_id -Value $StartupId -Force
    Add-Member -InputObject $Plan -MemberType NoteProperty -Name started_at_utc -Value $StartedAtUtc -Force
}

function Test-Beta2UiReadyLine([string]$Line) {
    $Line -match '\[ball-ready\].*收到 ball_ready'
}

function Assert-NoReparseAncestors([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Isolation path contains a reparse point: $current"
            }
        }
        $parent = Split-Path -Parent $current
        if (-not $parent -or $parent -eq $current) { break }
        $current = $parent
    }
}

$planPath = [IO.Path]::GetFullPath($PlanPath)
$plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json
$identityPath = [IO.Path]::GetFullPath([string]$plan.candidate_identity_path)
$identity = Get-Content -LiteralPath $identityPath -Raw | ConvertFrom-Json
$candidate = $identity.candidate

if ((Get-Sha256 $identityPath) -ne ([string]$plan.candidate_identity_sha256).ToUpperInvariant()) {
    throw 'Candidate identity file hash does not match the launch plan.'
}
if ([IO.Path]::GetFullPath([string]$candidate.executable) -ine [IO.Path]::GetFullPath([string]$plan.executable) -or
    [string]$candidate.exe_sha256 -ine [string]$plan.exe_sha256 -or
    [IO.Path]::GetFullPath([string]$candidate.pdb) -ine [IO.Path]::GetFullPath([string]$plan.pdb) -or
    [string]$candidate.pdb_sha256 -ine [string]$plan.pdb_sha256) {
    throw 'Launch plan candidate paths or hashes differ from candidate-identity.json.'
}
if (-not $candidate.pdb_id_matches_exe_codeview -or
    ("{0}/{1}" -f $candidate.pdb_guid, $candidate.pdb_age) -ine [string]$plan.pdb_guid_age -or
    [string]$identity.isolation.identifier -ine [string]$plan.app_identifier) {
    throw 'PDB identity or isolated app identifier does not match the launch plan.'
}
if ((Get-Sha256 ([string]$plan.executable)) -ne ([string]$plan.exe_sha256).ToUpperInvariant() -or
    (Get-Sha256 ([string]$plan.pdb)) -ne ([string]$plan.pdb_sha256).ToUpperInvariant()) {
    throw 'Frozen EXE or PDB SHA256 has changed.'
}

$arguments = @($plan.arguments)
$root = [IO.Path]::GetFullPath([string]$plan.isolation_root)
if ($arguments.Count -ne 2 -or $arguments[0] -cne '--beta2-isolated-data-root' -or
    [IO.Path]::GetFullPath([string]$arguments[1]) -ine $root) {
    throw 'Launch arguments must contain only the isolation switch and the planned absolute root.'
}
if (-not [IO.Path]::IsPathRooted($root) -or (Test-Path -LiteralPath $root)) {
    throw 'Isolation root must be an absolute path that does not exist before startup.'
}
Assert-NoReparseAncestors $root
if ([IO.Path]::GetFullPath([string]$plan.working_directory) -ine [IO.Path]::GetFullPath((Split-Path -Parent ([string]$plan.executable)))) {
    throw 'Working directory must be the frozen candidate directory.'
}
if (-not $plan.environment_policy.remove_all_inherited_webview2_variables -or
    $plan.environment_policy.set_webview2_variables -ne $false -or
    $plan.environment_policy.set_temp_or_tmp -ne $false) {
    throw 'Launch plan must remove inherited WEBVIEW2_* variables and set no WEBVIEW2_*, TEMP, or TMP variables.'
}
$diagnostic = $plan.diagnostic_environment
if ($null -eq $diagnostic -or [string]$diagnostic.variable -cne 'DSFLOAT_DIAG' -or
    [string]$diagnostic.value -cne '1' -or [string]$diagnostic.expected_level -cne 'Meta') {
    throw 'Launch plan must opt into redacted Release diagnostics with DSFLOAT_DIAG=1 (Meta).'
}
$diagnosticLog = [IO.Path]::GetFullPath([string]$plan.diagnostic_log)
$expectedDiagnosticLog = [IO.Path]::GetFullPath((Join-Path $root 'diagnostics\d1-debug.log'))
if ($diagnosticLog -ine $expectedDiagnosticLog -or
    -not $diagnosticLog.StartsWith($root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Diagnostic log must resolve to diagnostics\d1-debug.log inside the isolation root.'
}
$startupEnvironment = $plan.startup_environment
if ($null -ne $startupEnvironment) {
    $injectedSettings = @($startupEnvironment.PSObject.Properties | Where-Object {
        $name = [string]$_.Name
        $isWebView2Variable = $name.StartsWith('WEBVIEW2_', [StringComparison]::OrdinalIgnoreCase)
        $isWebView2Assignment = $name.StartsWith('process_sets_WEBVIEW2_', [StringComparison]::OrdinalIgnoreCase)
        ($isWebView2Variable -or $isWebView2Assignment) -and $null -ne $_.Value -and $_.Value -ne $false
    })
    if ($injectedSettings.Count -ne 0) {
        throw 'Launch plan contains a forbidden WEBVIEW2_* environment override.'
    }
}

$processName = [IO.Path]::GetFileNameWithoutExtension([string]$plan.executable)
$conflicts = @(Get-Process -Name $processName -ErrorAction SilentlyContinue | Select-Object Id, ProcessName, StartTime, Path)
if ($conflicts.Count -gt 0) {
    throw "Conflicting assistant process exists: $($conflicts | ConvertTo-Json -Compress)"
}

$startInfo = [Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = [IO.Path]::GetFullPath([string]$plan.executable)
$startInfo.WorkingDirectory = [IO.Path]::GetFullPath([string]$plan.working_directory)
$startInfo.UseShellExecute = $false
$startInfo.Arguments = '--beta2-isolated-data-root "' + $root + '"'
$startInfo.EnvironmentVariables['DSFLOAT_DIAG'] = [string]$diagnostic.value
$removedEnvironment = @()
foreach ($key in @($startInfo.EnvironmentVariables.Keys)) {
    if ($key.ToString().StartsWith('WEBVIEW2_', [StringComparison]::OrdinalIgnoreCase)) {
        $startInfo.EnvironmentVariables.Remove([string]$key)
        $removedEnvironment += [string]$key
    }
}
$childWebView2Variables = @($startInfo.EnvironmentVariables.Keys | Where-Object {
    ([string]$_).StartsWith('WEBVIEW2_', [StringComparison]::OrdinalIgnoreCase)
})
if ($childWebView2Variables.Count -ne 0) {
    throw "WEBVIEW2_* variables remain in the candidate environment: $($childWebView2Variables -join ', ')"
}

if ($PreflightOnly) {
    [ordered]@{
        preflight_passed = $true
        candidate_started = $false
        candidate_hashes_verified = $true
        identity_and_plan_match = $true
        isolation_root_absent = $true
        conflicting_processes = $conflicts
        removed_inherited_webview2_variables = @($removedEnvironment | Sort-Object -Unique)
        child_webview2_variables = $childWebView2Variables
        child_system_root_present = $startInfo.EnvironmentVariables.ContainsKey('SystemRoot')
        child_path_present = $startInfo.EnvironmentVariables.ContainsKey('Path')
        child_diagnostic_value = $startInfo.EnvironmentVariables['DSFLOAT_DIAG']
        diagnostic_level = [string]$diagnostic.expected_level
        diagnostic_log = $diagnosticLog
        diagnostic_log_within_isolation_root = $diagnosticLog.StartsWith($root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)
        arguments = $arguments
    } | ConvertTo-Json -Depth 8 -Compress
    exit 0
}

if ($plan.launch_authorized -ne $true -or [int]$plan.max_launches -ne 1 -or [int]$plan.launches_so_far -ne 0) {
    throw 'This run plan is not authorized for exactly one unused launch.'
}

$nativeObserver = @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public sealed class Beta2WindowRecord {
    public long hwnd { get; set; }
    public uint pid { get; set; }
    public bool valid { get; set; }
    public bool visible { get; set; }
    public string title { get; set; }
}
public sealed class Beta2ProcessRecord {
    public uint ProcessId { get; set; }
    public uint ParentProcessId { get; set; }
    public string Name { get; set; }
}
public static class Beta2Win32Windows {
    private delegate bool EnumProc(IntPtr hwnd, IntPtr state);
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    private struct PROCESSENTRY32 {
        public uint dwSize;
        public uint cntUsage;
        public uint th32ProcessID;
        public IntPtr th32DefaultHeapID;
        public uint th32ModuleID;
        public uint cntThreads;
        public uint th32ParentProcessID;
        public int pcPriClassBase;
        public uint dwFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string szExeFile;
    }
    private const uint TH32CS_SNAPPROCESS = 0x00000002;
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr state);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maxCount);
    [DllImport("kernel32.dll", SetLastError=true)] private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);
    [DllImport("kernel32.dll", EntryPoint="Process32FirstW", CharSet=CharSet.Unicode, SetLastError=true)] private static extern bool Process32First(IntPtr snapshot, ref PROCESSENTRY32 entry);
    [DllImport("kernel32.dll", EntryPoint="Process32NextW", CharSet=CharSet.Unicode, SetLastError=true)] private static extern bool Process32Next(IntPtr snapshot, ref PROCESSENTRY32 entry);
    [DllImport("kernel32.dll", SetLastError=true)] private static extern bool CloseHandle(IntPtr handle);
    public static Beta2WindowRecord[] ForProcess(uint targetPid) {
        var rows = new List<Beta2WindowRecord>();
        EnumWindows((hwnd, state) => {
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            if (owner == targetPid) {
                var title = new StringBuilder(512);
                GetWindowText(hwnd, title, title.Capacity);
                rows.Add(new Beta2WindowRecord { hwnd = hwnd.ToInt64(), pid = owner, valid = IsWindow(hwnd), visible = IsWindowVisible(hwnd), title = title.ToString() });
            }
            return true;
        }, IntPtr.Zero);
        return rows.ToArray();
    }
    public static Beta2ProcessRecord[] DescendantsOf(uint rootPid) {
        IntPtr snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if (snapshot == new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
        var all = new List<Beta2ProcessRecord>();
        try {
            var entry = new PROCESSENTRY32();
            entry.dwSize = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
            if (!Process32First(snapshot, ref entry)) throw new Win32Exception(Marshal.GetLastWin32Error());
            do {
                all.Add(new Beta2ProcessRecord {
                    ProcessId = entry.th32ProcessID,
                    ParentProcessId = entry.th32ParentProcessID,
                    Name = entry.szExeFile
                });
                entry.dwSize = (uint)Marshal.SizeOf(typeof(PROCESSENTRY32));
            } while (Process32Next(snapshot, ref entry));
        } finally {
            CloseHandle(snapshot);
        }
        var ids = new HashSet<uint> { rootPid };
        var descendants = new List<Beta2ProcessRecord>();
        bool changed;
        do {
            changed = false;
            foreach (var item in all) {
                if (!ids.Contains(item.ProcessId) && ids.Contains(item.ParentProcessId)) {
                    ids.Add(item.ProcessId);
                    descendants.Add(item);
                    changed = true;
                }
            }
        } while (changed);
        return descendants.ToArray();
    }
}
'@
Add-Type -TypeDefinition $nativeObserver -ErrorAction Stop

$runId = [Guid]::NewGuid().ToString('N')
$evidenceRoot = [IO.Path]::GetFullPath([string]$plan.evidence_root)
$runDirectory = Join-Path $evidenceRoot ("native-run-" + $runId)
if (Test-Path -LiteralPath $runDirectory) { throw 'Run evidence directory already exists.' }
New-Item -ItemType Directory -Path $runDirectory | Out-Null
$formalPaths = @($plan.formal_data_files | ForEach-Object { [string]$_ })
$runKeyPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runKeyName = 'deepseek-float'
$baseline = [ordered]@{
    captured_at = [DateTime]::UtcNow.ToString('o')
    formal_files = Get-FileSnapshot $formalPaths
    autostart_run_value = Get-RunValue $runKeyPath $runKeyName
    isolation_root = $root
    exe_sha256 = Get-Sha256 ([string]$plan.executable)
    pdb_sha256 = Get-Sha256 ([string]$plan.pdb)
}
$baselinePath = Join-Path $runDirectory 'preflight-baseline.json'
[IO.File]::WriteAllText($baselinePath, ($baseline | ConvertTo-Json -Depth 10))

# Consume this run's single-use authorization before process creation.
Set-Beta2PlanLaunchMetadata -Plan $plan -StartupId $runId -StartedAtUtc ([DateTime]::UtcNow.ToString('o'))
[IO.File]::WriteAllText($planPath, ($plan | ConvertTo-Json -Depth 20))

$eventsPath = Join-Path $runDirectory 'process-window-monitor.jsonl'
$diagnosticsPath = $diagnosticLog
$process = [Diagnostics.Process]::Start($startInfo)
if (-not $process) { throw 'Process.Start returned no process handle.' }
$pidValue = $process.Id
$knownChildren = [Collections.Generic.HashSet[int]]::new()
$readySeen = $false
$started = [ordered]@{
    event = 'process_started'
    started_at_utc = [DateTime]::UtcNow.ToString('o')
    startup_id = $runId
    pid = $pidValue
    exe_sha256 = $plan.exe_sha256
    removed_inherited_webview2_variables = @($removedEnvironment | Sort-Object -Unique)
    isolation_root = $root
    evidence_directory = $runDirectory
}
[IO.File]::WriteAllText($eventsPath, (($started | ConvertTo-Json -Compress) + [Environment]::NewLine))
Write-Output ($started | ConvertTo-Json -Compress)

function Get-DescendantProcesses([int]$RootPid) {
    [Beta2Win32Windows]::DescendantsOf([uint32]$RootPid)
}

while (-not $process.HasExited) {
    $children = @(Get-DescendantProcesses $pidValue)
    foreach ($child in $children) { [void]$knownChildren.Add([int]$child.ProcessId) }
    $ready = $false
    if (Test-Path -LiteralPath $diagnosticsPath -PathType Leaf) {
        $ready = [bool](Select-String -LiteralPath $diagnosticsPath -Pattern '\[ball-ready\].*收到 ball_ready' -Quiet -ErrorAction SilentlyContinue)
    }
    if ($ready -and -not $readySeen) { $readySeen = $true }
    $windows = @([Beta2Win32Windows]::ForProcess([uint32]$pidValue) | ForEach-Object {
        [ordered]@{ hwnd = $_.hwnd; pid = $_.pid; valid = $_.valid; visible = $_.visible; title = $_.title }
    })
    $event = [ordered]@{
        event = 'process_sample'
        captured_at_utc = [DateTime]::UtcNow.ToString('o')
        pid = $pidValue
        windows = $windows
        child_processes = @($children | ForEach-Object { [ordered]@{ pid = $_.ProcessId; parent_pid = $_.ParentProcessId; name = $_.Name } })
        ui_ready_signal_seen = $readySeen
    }
    [IO.File]::AppendAllText($eventsPath, (($event | ConvertTo-Json -Depth 8 -Compress) + [Environment]::NewLine))
    Start-Sleep -Milliseconds 500
    $process.Refresh()
}
$process.Refresh()
$exitCode = $process.ExitCode

$remainingChildren = @()
$deadline = [DateTime]::UtcNow.AddSeconds(10)
do {
    $remainingChildren = @($knownChildren | Where-Object {
        Get-Process -Id $_ -ErrorAction SilentlyContinue
    })
    if ($remainingChildren.Count -eq 0) { break }
    Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $deadline)

$after = [ordered]@{
    captured_at = [DateTime]::UtcNow.ToString('o')
    formal_files = Get-FileSnapshot $formalPaths
    autostart_run_value = Get-RunValue $runKeyPath $runKeyName
    isolation_files = @(Get-ChildItem -LiteralPath $root -File -Recurse -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    ui_ready_signal_seen = $readySeen
    main_process_pid = $pidValue
    main_process_exit_code = $exitCode
    known_webview_child_pids = @($knownChildren | Sort-Object)
    remaining_child_pids_after_10s = @($remainingChildren)
}
[IO.File]::WriteAllText((Join-Path $runDirectory 'post-run.json'), ($after | ConvertTo-Json -Depth 12))
$completed = [ordered]@{
    event = 'process_exited'
    captured_at_utc = [DateTime]::UtcNow.ToString('o')
    startup_id = $runId
    pid = $pidValue
    exit_code = $exitCode
    ui_ready_signal_seen = $readySeen
    known_child_pids = @($knownChildren | Sort-Object)
    remaining_child_pids_after_10s = @($remainingChildren)
    evidence_directory = $runDirectory
}
[IO.File]::AppendAllText($eventsPath, (($completed | ConvertTo-Json -Compress) + [Environment]::NewLine))
Write-Output ($completed | ConvertTo-Json -Compress)
