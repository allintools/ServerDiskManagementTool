param(
  [string]$AppExe = "src-tauri\target\release\ServerDiskManagementTool.exe",
  [string]$Passphrase = "nova-rack-test-passphrase"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$app = if ([System.IO.Path]::IsPathRooted($AppExe)) { $AppExe } else { Join-Path $root $AppExe }
if (-not (Test-Path $app)) {
  throw "Nova Rack binary not found: $app`nBuild it first: npm run desktop:build (or npx tauri build --no-bundle)"
}

$temp = Join-Path $env:TEMP ("nova-rack-askpass-" + [guid]::NewGuid().ToString("N"))
$key = Join-Path $temp "id_ed25519"
$secret = Join-Path $temp "secret"
New-Item -ItemType Directory -Force -Path $temp | Out-Null

try {
  & ssh-keygen -q -t ed25519 -N $Passphrase -f $key
  if ($LASTEXITCODE -ne 0) { throw "ssh-keygen failed to create the test key" }
  Set-Content -Path $secret -Value $Passphrase -NoNewline -Encoding ascii

  $env:SSH_ASKPASS = $app
  $env:SSH_ASKPASS_REQUIRE = "force"
  $env:DISPLAY = ":0"
  $env:NOVA_RACK_ASKPASS = "1"
  $env:NOVA_RACK_ASKPASS_SECRET = $secret

  $start = New-Object System.Diagnostics.ProcessStartInfo
  $start.FileName = (Get-Command ssh-keygen).Source
  $start.Arguments = "-y -f `"$key`""
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.UseShellExecute = $false
  $process = [System.Diagnostics.Process]::Start($start)
  $process.StandardInput.Close()
  if (-not $process.WaitForExit(15000)) {
    $process.Kill()
    throw "ssh-keygen did not finish; askpass was not invoked"
  }
  $out = $process.StandardOutput.ReadToEnd().Trim()
  $err = $process.StandardError.ReadToEnd().Trim()
  if ($process.ExitCode -ne 0) {
    throw "ssh-keygen -y failed (exit $($process.ExitCode)): $err"
  }
  $pub = (Get-Content ($key + ".pub") -Raw).Trim().Split(" ")[1]
  if (-not $out.Contains($pub)) {
    throw "askpass output did not match the public key. stdout=$out stderr=$err"
  }
  Write-Host "Windows askpass passed: encrypted key unlocked through the app executable"
} finally {
  Remove-Item Env:SSH_ASKPASS -ErrorAction SilentlyContinue
  Remove-Item Env:SSH_ASKPASS_REQUIRE -ErrorAction SilentlyContinue
  Remove-Item Env:DISPLAY -ErrorAction SilentlyContinue
  Remove-Item Env:NOVA_RACK_ASKPASS -ErrorAction SilentlyContinue
  Remove-Item Env:NOVA_RACK_ASKPASS_SECRET -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}
