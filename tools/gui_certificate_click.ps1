param(
    [Parameter(Mandatory=$true)][long]$ObsHandle,
    [Parameter(Mandatory=$true)][int]$ObsProcessId,
    [Parameter(Mandatory=$true)][string]$SourceName,
    [switch]$Approve,
    [switch]$ExpectError,
    [int]$Repeat = 1
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new()
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Find-Elements($root) {
    # OBS rebuilds its Qt property widgets asynchronously. UI Automation can
    # briefly report stale elements during that legitimate rebuild.
    try {
        $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]::new($ObsHandle))
        $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition)
    } catch {
        @()
    }
}

$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]::new($ObsHandle))
$item = $null
$propertiesButton = $null
foreach ($element in (Find-Elements $root)) {
    $current = $element.Current
    if ($current.Name -eq $SourceName -and
        $current.ControlType -eq [System.Windows.Automation.ControlType]::ListItem) { $item = $element }
    if ($current.Name -eq 'Properties' -and $current.AutomationId -like '*sourcesToolbar*') { $propertiesButton = $element }
}
if (-not $item -or -not $propertiesButton) { throw 'Could not locate the isolated test source' }
$item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
Start-Sleep -Milliseconds 250
$propertiesButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()

for ($iteration=0; $iteration -lt $Repeat; $iteration++) {
    $fetch = $null
    for ($attempt=0; $attempt -lt 40; $attempt++) {
        foreach ($element in (Find-Elements $root)) {
            try {
                if ($element.Current.Name -eq 'Fetch device certificate' -and
                    $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button) { $fetch = $element; break }
            } catch { continue }
        }
        if ($fetch) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $fetch) { throw 'Certificate button not found in properties dialog' }
    Write-Output "GUI_TEST: invoking fetch button ($($iteration + 1)/$Repeat)"
    $fetch.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 70
}

if ($Approve) {
    $approval = $null
    $fingerprintSeen = $false
    for ($attempt=0; $attempt -lt 120; $attempt++) {
        foreach ($element in (Find-Elements $root)) {
            try {
                if ($element.Current.Name -like '*SHA-256 fingerprint*') { $fingerprintSeen = $true }
                if ($element.Current.Name -eq 'I verified it: use certificate' -and $element.Current.IsEnabled) {
                    $approval = $element; break
                }
            } catch { continue }
        }
        if ($approval) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $approval) { throw 'Approval button never became enabled' }
    if (-not $fingerprintSeen) { throw 'Certificate fingerprint was not displayed' }
    Write-Output 'GUI_TEST: fingerprint shown; invoking approval button'
    $approval.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
Start-Sleep -Milliseconds 750
Write-Output 'GUI_TEST: clicked button and OBS UI automation still responds'

if ($ExpectError) {
    $foundError = $false
    for ($attempt=0; $attempt -lt 40; $attempt++) {
        foreach ($element in (Find-Elements $root)) {
            try {
                if ($element.Current.Name -like '*Certificate fetch failed*') {
                    $foundError = $true; break
                }
            } catch { continue }
        }
        if ($foundError) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $foundError) { throw 'Expected certificate error was not displayed' }
    Write-Output 'GUI_TEST: expected error displayed without a crash'
}

if ($Approve) {
    $ok = $null
    for ($attempt=0; $attempt -lt 40; $attempt++) {
        foreach ($element in (Find-Elements $root)) {
            try {
                if ($element.Current.Name -eq 'OK' -and $element.Current.AutomationId -like '*OBSBasicProperties*') {
                    $ok = $element; break
                }
            } catch { continue }
        }
        if ($ok) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ok) { throw 'Properties OK button not found' }
    $ok.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $closed = $false
    for ($attempt=0; $attempt -lt 60; $attempt++) {
        $hasDialog = $false
        foreach ($element in (Find-Elements $root)) {
            try {
                if ($element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Window -and
                    $element.Current.Name -like "Properties for*") { $hasDialog = $true; break }
            } catch { continue }
        }
        if (-not $hasDialog) { $closed = $true; break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $closed) { throw 'Properties dialog did not close after OK' }
    Write-Output 'GUI_TEST: approved settings saved via OK'
}
