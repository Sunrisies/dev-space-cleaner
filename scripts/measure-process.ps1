param([string]$ProcessName = 'huanj')
$ErrorActionPreference = 'Stop'
$qaRoot = @(Get-Process -Name $ProcessName)
if ($qaRoot.Count -ne 1) { throw 'Expected exactly one application process' }
$qaNodes = @(Get-CimInstance Win32_Process)
$qaIds = @($qaRoot[0].Id)
do {
    $qaNext = @($qaNodes | Where-Object {
        $_.ParentProcessId -in $qaIds -and $_.ProcessId -notin $qaIds
    } | Select-Object -ExpandProperty ProcessId)
    $qaIds += $qaNext
} while ($qaNext.Count -gt 0)
$qaProcesses = @(Get-Process -Id $qaIds -ErrorAction SilentlyContinue)
[pscustomobject]@{
    Processes = $qaProcesses.Count
    SumWorkingSetMiB = [math]::Round(($qaProcesses | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
    SumPrivateBytesMiB = [math]::Round(($qaProcesses | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
} | Format-List
