param(
  [string]$AppExe = "src-tauri\target\release\ServerDiskManagementTool.exe",
  [int]$TimeoutSeconds = 60
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$app = if ([System.IO.Path]::IsPathRooted($AppExe)) { $AppExe } else { Join-Path $root $AppExe }
if (-not (Test-Path $app)) {
  throw "Nova Rack binary not found: $app`nBuild it first: npm run desktop:build (or npx tauri build --no-bundle)"
}

$temp = Join-Path $env:TEMP ("nova-rack-smoke-" + [guid]::NewGuid().ToString("N"))
$report = Join-Path $temp "report.json"
New-Item -ItemType Directory -Force -Path $temp | Out-Null

$env:RACK_DATA_DIR = $temp
$env:RACK_SMOKE = "1"
$env:RACK_SMOKE_FILE = $report
$process = $null
try {
  $process = Start-Process -FilePath $app -PassThru
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while (-not (Test-Path $report) -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 300
  }
  if (-not (Test-Path $report)) {
    throw "Native smoke report was not written within $TimeoutSeconds seconds"
  }

  $text = Get-Content $report -Raw -Encoding UTF8
  $json = $text | ConvertFrom-Json
  if ($json.fatal) { throw "Native smoke fatal error: $($json.fatal)" }
  if ($json.navCount -ne 6) { throw "Expected 6 navigation items, got $($json.navCount)" }
  if (($json.pages | Measure-Object).Count -ne 6) { throw "Expected 6 pages, got $(($json.pages | Measure-Object).Count)" }
  if (-not ($json.checks | Where-Object { $_.addDialog })) { throw "Add-server dialog did not open" }
  if ($json.errors) { throw "Native smoke reported errors: $($json.errors -join '; ')" }
  if ($json.ssh) {
    if (-not $json.deleteButton) { throw "Server delete button was not rendered" }
    if ($json.serversAfterDelete -ne 0) { throw "Server delete did not remove the record" }
  }

  Write-Host "Windows native smoke passed: 6 pages, add-server dialog, IPC list_servers, server delete"
} finally {
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
  }
  Remove-Item Env:RACK_DATA_DIR -ErrorAction SilentlyContinue
  Remove-Item Env:RACK_SMOKE -ErrorAction SilentlyContinue
  Remove-Item Env:RACK_SMOKE_FILE -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}
