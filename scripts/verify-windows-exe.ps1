<#
.SYNOPSIS
    Checks that the shipped executable really does both of its jobs.

.DESCRIPTION
    The release build is a GUI-subsystem executable, so that double-clicking it
    does not flash a console window. The Rust test suite runs in debug, where
    that flag does not apply — so the one configuration that actually ships is
    the one no test covers.

    This closes that gap. It checks the file exists and is one file, that the
    console subcommands answer, and — the part that matters — that MCP mode
    still speaks the protocol over inherited pipes with no console attached.

.PARAMETER Exe
    Path to safe-invest.exe.

.PARAMETER MaxSizeMb
    Fail if the executable is larger than this. Guards against something bulky
    being pulled in without anyone noticing.
#>

[CmdletBinding()]
param(
    [string] $Exe = "target/release/safe-invest.exe",
    [double] $MaxSizeMb = 40
)

$ErrorActionPreference = "Stop"

function Step($message) { Write-Host "==> $message" }

# ------------------------------------------------------------------ the file

if (-not (Test-Path $Exe)) { throw "$Exe est introuvable" }

$size = [math]::Round((Get-Item $Exe).Length / 1MB, 2)
Step "safe-invest.exe : $size Mo"
if ($size -gt $MaxSizeMb) {
    throw "l'exécutable fait $size Mo, au-delà de la limite de $MaxSizeMb Mo"
}

$hash = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLower()
Step "SHA-256 : $hash"

# ------------------------------------------------------- console subcommands

$data = Join-Path $env:RUNNER_TEMP "safe-invest-verify"
New-Item -ItemType Directory -Force -Path $data | Out-Null

<#
.SYNOPSIS
    Runs a console subcommand and returns what it printed.

.DESCRIPTION
    A GUI-subsystem executable is *not* awaited by the shell: `& $exe --version`
    returns at once and leaves `$LASTEXITCODE` unset, so the obvious check
    reports a failure that never happened. `Start-Process -Wait` waits properly
    and hands back a real exit code, and redirecting the output lets this script
    assert that something was actually printed — a stronger check than the exit
    code alone, and the one that would have caught the null-handle panic.
#>
function Invoke-Subcommand([string[]] $Arguments) {
    $out = Join-Path $data "out.txt"
    $err = Join-Path $data "err.txt"

    $process = Start-Process -FilePath (Resolve-Path $Exe).Path `
        -ArgumentList $Arguments -Wait -PassThru -NoNewWindow `
        -RedirectStandardOutput $out -RedirectStandardError $err

    $text = if (Test-Path $out) { (Get-Content $out -Raw) } else { "" }
    $errors = if (Test-Path $err) { (Get-Content $err -Raw) } else { "" }

    if ($process.ExitCode -ne 0) {
        throw "$($Arguments -join ' ') a renvoyé $($process.ExitCode)`n$errors"
    }
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "$($Arguments -join ' ') n'a rien affiché — la sortie standard est-elle utilisable ?"
    }
    return $text.Trim()
}

Step "--version"
$version = Invoke-Subcommand @("--version")
Write-Host "    $version"
if ($version -notmatch "^Safe Invest \d+\.\d+\.\d+") {
    throw "--version a répondu quelque chose d'inattendu : $version"
}

Step "doctor"
$report = Invoke-Subcommand @("doctor", "--data-dir", $data, "--demo")
foreach ($line in $report -split "`n" | Select-Object -First 6) { Write-Host "    $line" }
if ($report -notmatch "moteur web") {
    throw "doctor n'a pas produit son diagnostic complet"
}

# ------------------------------------------------------------------ MCP mode

Step "poignée de main MCP sur les tuyaux"

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = (Resolve-Path $Exe).Path
$psi.Arguments = "mcp --demo --data-dir `"$data`""
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true

$server = [System.Diagnostics.Process]::Start($psi)
try {
    $request = '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"verify","version":"1"}}}'
    $server.StandardInput.WriteLine($request)
    $server.StandardInput.Flush()

    # A GUI-subsystem process with no console must still answer here: the
    # client hands it pipes, and inherited pipes are what stdio means.
    $read = $server.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(20000)) {
        throw "aucune réponse du serveur MCP en 20 s — l'exécutable en sous-système « windows » ne lit pas ses tuyaux"
    }

    $reply = $read.Result
    if ([string]::IsNullOrWhiteSpace($reply)) { throw "le serveur MCP a fermé sa sortie sans répondre" }

    $parsed = $reply | ConvertFrom-Json
    if ($parsed.error) { throw "le serveur MCP a répondu par une erreur : $($parsed.error.message)" }
    if ($parsed.result.serverInfo.name -ne "safe-invest") {
        throw "réponse inattendue à initialize : $reply"
    }

    Step "le serveur MCP répond : $($parsed.result.serverInfo.name) $($parsed.result.serverInfo.version)"
}
finally {
    if (-not $server.HasExited) { $server.Kill() }
    $server.WaitForExit(5000) | Out-Null
}

Write-Host ""
Write-Host "L'exécutable unique fait bien ses deux métiers."

if ($env:GITHUB_OUTPUT) {
    "size=$size"     | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
    "sha256=$hash"   | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
}
