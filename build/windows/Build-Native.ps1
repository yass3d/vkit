[CmdletBinding()]
param(
    [ValidateSet("release", "release-small")]
    [string]$Profile = "release-small",
    [string]$OutputRoot,
    [int]$MaximumMiB = 26,
    [string]$TargetTriple = "x86_64-pc-windows-msvc",
    [string]$CargoTargetDir,
    [ValidateRange(1, 100)]
    [int]$DiagnosticSessionRetention = 12,
    [switch]$SkipTests,
    [switch]$EnableLocalSigning,
    [switch]$DisableLocalSigning
)

$ErrorActionPreference = "Stop"
$ProjectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$NativeRoot = Join-Path $ProjectRoot "native"
$DefaultCargoTargetDir = Join-Path ([System.IO.Path]::GetTempPath()) "vkit-native-target"
if (-not $CargoTargetDir) {
    $CargoTargetDir = if ($env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR
    } else {
        $DefaultCargoTargetDir
    }
}
$CargoTargetDir = [System.IO.Path]::GetFullPath($CargoTargetDir)
$DefaultOutputRoot = Join-Path $ProjectRoot "dist\native"
if (-not $OutputRoot) {
    $OutputRoot = $DefaultOutputRoot
}
$OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$Destination = Join-Path $OutputRoot "Vkit.exe"
$PendingRoot = [System.IO.Path]::GetFullPath(("$OutputRoot-pending"))
$PendingDestination = Join-Path $PendingRoot "Vkit.exe"
$DestinationLockedBeforeBuild = $false
if (Test-Path -LiteralPath $Destination -PathType Leaf) {
    $LockProbe = $null
    try {
        $LockProbe = [System.IO.File]::Open(
            $Destination,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
    } catch {
        $DestinationLockedBeforeBuild = $true
        Write-Warning "The current Vkit.exe is in use. This build will be preserved under '$PendingRoot' and can be promoted without recompiling."
    } finally {
        if ($LockProbe) {
            $LockProbe.Dispose()
        }
    }
}
$Manifest = Join-Path $NativeRoot "Cargo.toml"
$ReceiptDir = Join-Path $ProjectRoot "logs"
$Signer = Join-Path $PSScriptRoot "Sign-EphemeralCodeSigning.ps1"
$BuildSession = "{0}-{1}" -f (Get-Date -Format "yyyyMMdd-HHmmss"), $PID
$UseLocalSigning = $EnableLocalSigning -and -not $DisableLocalSigning

if (-not (Test-Path -LiteralPath $Manifest -PathType Leaf)) {
    throw "Native Cargo workspace was not found: $Manifest"
}
if ($MaximumMiB -le 0) {
    throw "MaximumMiB must be positive."
}
if ($EnableLocalSigning -and $DisableLocalSigning) {
    throw "EnableLocalSigning and DisableLocalSigning cannot be used together."
}
if ($UseLocalSigning -and -not (Test-Path -LiteralPath $Signer -PathType Leaf)) {
    throw "Local signing helper was not found: $Signer"
}

New-Item -ItemType Directory -Path $ReceiptDir -Force | Out-Null
$CargoCommand = Get-Command cargo.exe -ErrorAction Stop
$CargoExe = $CargoCommand.Source

function Invoke-CargoCaptured {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [int]$Attempt
    )

    $StdoutPath = Join-Path $ReceiptDir ("cargo-{0}-{1}-attempt-{2}.stdout.log" -f $Label, $BuildSession, $Attempt)
    $StderrPath = Join-Path $ReceiptDir ("cargo-{0}-{1}-attempt-{2}.stderr.log" -f $Label, $BuildSession, $Attempt)
    $QuotedArguments = @(
        foreach ($Argument in $Arguments) {
            if ($Argument -notmatch '[\s"]') {
                $Argument
                continue
            }
            '"{0}"' -f ($Argument -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1')
        }
    )
    $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $StartInfo.FileName = $CargoExe
    $StartInfo.Arguments = $QuotedArguments -join " "
    $StartInfo.WorkingDirectory = $NativeRoot
    $StartInfo.UseShellExecute = $false
    $StartInfo.CreateNoWindow = $true
    $StartInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $StartInfo.RedirectStandardOutput = $true
    $StartInfo.RedirectStandardError = $true
    $Process = [System.Diagnostics.Process]::new()
    $Process.StartInfo = $StartInfo
    if (-not $Process.Start()) {
        throw "Cargo $Label could not be started."
    }
    $StdoutTask = $Process.StandardOutput.ReadToEndAsync()
    $StderrTask = $Process.StandardError.ReadToEndAsync()
    $Process.WaitForExit()
    $CapturedStdout = $StdoutTask.GetAwaiter().GetResult()
    $CapturedStderr = $StderrTask.GetAwaiter().GetResult()
    [System.IO.File]::WriteAllText($StdoutPath, $CapturedStdout)
    [System.IO.File]::WriteAllText($StderrPath, $CapturedStderr)

    $Stdout = if (Test-Path -LiteralPath $StdoutPath) {
        Get-Content -LiteralPath $StdoutPath -Raw
    } else {
        ""
    }
    $Stderr = if (Test-Path -LiteralPath $StderrPath) {
        Get-Content -LiteralPath $StderrPath -Raw
    } else {
        ""
    }
    if ($Stdout) {
        Write-Host $Stdout.TrimEnd()
    }
    if ($Stderr) {
        Write-Host $Stderr.TrimEnd()
    }

    [pscustomobject]@{
        ExitCode = $Process.ExitCode
        Text = ($Stdout + [Environment]::NewLine + $Stderr)
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
    }
}

function Invoke-CargoWithAppControlRetry {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    $MaximumAttempts = 2
    for ($Attempt = 1; $Attempt -le $MaximumAttempts; $Attempt++) {
        Write-Host ("Cargo {0} (attempt {1}/{2})" -f $Label, $Attempt, $MaximumAttempts) -ForegroundColor Cyan
        $Result = Invoke-CargoCaptured -Arguments $Arguments -Label $Label -Attempt $Attempt
        if ($Result.ExitCode -eq 0) {
            return
        }

        $WasApplicationControl = $Result.Text -match "(?i)(os error 4551|error 4551|application control|code integrity|blocked by (?:group policy|your system administrator))"
        if (-not $WasApplicationControl) {
            throw ("Cargo {0} failed with exit code {1}. Logs: {2}, {3}" -f $Label, $Result.ExitCode, $Result.StdoutPath, $Result.StderrPath)
        }

        if ($Attempt -lt $MaximumAttempts) {
            Write-Warning "Windows Application Control blocked Cargo (4551). Retrying once without modifying or signing Cargo outputs."
            Start-Sleep -Seconds 2
            continue
        }
    }

    throw ("Cargo {0} remained blocked by Windows Application Control after {1} non-mutating attempts. No security policy, trust store, toolchain, registry source, or Cargo intermediate was changed. Use a policy-approved build machine or pass -CargoTargetDir with a verified successful cache. Logs: {2}" -f $Label, $MaximumAttempts, $ReceiptDir)
}

function Remove-ExpiredCargoDiagnostics {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Directory,
        [Parameter(Mandatory = $true)]
        [int]$KeepSessions
    )

    $SessionFiles = @(
        Get-ChildItem -LiteralPath $Directory -File -Filter "cargo-*.log" -ErrorAction SilentlyContinue |
            ForEach-Object {
                if ($_.Name -match "^cargo-.+-(\d{8}-\d{6}-\d+)-attempt-\d+\.(?:stdout|stderr)\.log$") {
                    [pscustomobject]@{
                        Session = $Matches[1]
                        File = $_
                    }
                }
            }
    )
    if ($SessionFiles.Count -eq 0) {
        return
    }

    $Expired = @(
        $SessionFiles |
            Group-Object Session |
            Sort-Object { ($_.Group.File | Measure-Object LastWriteTime -Maximum).Maximum } -Descending |
            Select-Object -Skip $KeepSessions
    )
    foreach ($Group in $Expired) {
        foreach ($Entry in $Group.Group) {
            Remove-Item -LiteralPath $Entry.File.FullName -Force -ErrorAction SilentlyContinue
        }
    }
}

function Find-DumpBin {
    $Command = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($Command) {
        return $Command.Source
    }

    $Roots = @(
        $env:VKIT_MSVC_ROOT,
        (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio"),
        (Join-Path $env:ProgramFiles "Microsoft Visual Studio")
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) }

    foreach ($Root in $Roots) {
        $Found = Get-ChildItem -LiteralPath $Root -Recurse -Filter "dumpbin.exe" -File -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match "Hostx64\\x64\\dumpbin\.exe$" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($Found) {
            return $Found.FullName
        }
    }
    return $null
}

function Get-PeImports {
    param([Parameter(Mandatory = $true)][string]$Executable)

    $DumpBin = Find-DumpBin
    if (-not $DumpBin) {
        throw "dumpbin.exe is required for the native dependency audit. Install the MSVC build tools or add dumpbin.exe to PATH."
    }
    $Output = & $DumpBin /DEPENDENTS $Executable 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin dependency audit failed with exit code $LASTEXITCODE"
    }
    @(
        $Output |
            ForEach-Object { $_.ToString().Trim() } |
            Where-Object { $_ -match "^[A-Za-z0-9_.-]+\.dll$" } |
            ForEach-Object { $_.ToLowerInvariant() } |
            Select-Object -Unique
    )
}

$PreviousRustFlags = $env:RUSTFLAGS
$PreviousCargoTargetDir = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = $CargoTargetDir
    if ($env:RUSTFLAGS -notmatch "(?i)target-feature\s*=\s*[^\r\n]*\+crt-static") {
        $env:RUSTFLAGS = (($env:RUSTFLAGS + " -C target-feature=+crt-static").Trim())
    }

    $CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
    $RustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }
    foreach ($Remap in @("$CargoHome=cargo", "$RustupHome=rustup")) {
        if ($env:RUSTFLAGS -notmatch [regex]::Escape($Remap)) {
            $env:RUSTFLAGS = (($env:RUSTFLAGS + " --remap-path-prefix=$Remap").Trim())
        }
    }

    if (-not $SkipTests) {
        Invoke-CargoWithAppControlRetry `
            -Arguments @("test", "--workspace", "--locked", "--target", $TargetTriple) `
            -Label "native-tests"
    }

    Invoke-CargoWithAppControlRetry `
        -Arguments @(
            "build", "--profile", $Profile, "-p", "vkit-app",
            "--bin", "Vkit", "--locked", "--target", $TargetTriple
        ) `
        -Label ("native-{0}" -f $Profile)
}
finally {
    $env:RUSTFLAGS = $PreviousRustFlags
    $env:CARGO_TARGET_DIR = $PreviousCargoTargetDir
}

$SourceExe = Join-Path $CargoTargetDir "$TargetTriple\$Profile\Vkit.exe"
if (-not (Test-Path -LiteralPath $SourceExe -PathType Leaf)) {
    throw "Native Vkit executable was not produced: $SourceExe"
}
$SourceItem = Get-Item -LiteralPath $SourceExe

$StageRoot = Join-Path $ReceiptDir (".native-stage-{0}" -f $PID)
if (Test-Path -LiteralPath $StageRoot) {
    throw "Native staging directory already exists: $StageRoot"
}
$StagedExe = Join-Path $StageRoot "Vkit.exe"
$PublishState = "published"
$PreserveStageOnFailure = $false

try {
    New-Item -ItemType Directory -Path $StageRoot | Out-Null
    Copy-Item -LiteralPath $SourceExe -Destination $StagedExe

    if ($UseLocalSigning) {
        & $Signer -Path $StagedExe | Out-Host
    }

    $StagedItem = Get-Item -LiteralPath $StagedExe
    $LimitBytes = [int64]$MaximumMiB * 1MB
    if ($StagedItem.Length -gt $LimitBytes) {
        throw ("Vkit.exe is {0:N2} MiB, above the {1} MiB release gate." -f ($StagedItem.Length / 1MB), $MaximumMiB)
    }

    $Imports = @(Get-PeImports -Executable $StagedExe)
    if ($Imports.Count -eq 0) {
        throw "Native dependency audit returned no imported DLLs. Refusing to publish an unaudited EXE."
    }
    $SystemDirectory = [Environment]::GetFolderPath([Environment+SpecialFolder]::System)
    $ExternalImports = @(
        $Imports | Where-Object {
            $_ -notmatch "^(api-ms-win-|ext-ms-win-)" -and
            -not (Test-Path -LiteralPath (Join-Path $SystemDirectory $_) -PathType Leaf)
        }
    )
    if ($ExternalImports.Count -gt 0) {
        throw ("Native Vkit has non-Windows runtime imports: {0}" -f ($ExternalImports -join ", "))
    }
    $DynamicCrtImports = @(
        $Imports | Where-Object {
            $_ -match "^(vcruntime\d*|msvcp\d*|ucrtbase|concrt\d*|msvcr\d*)\.dll$"
        }
    )
    if ($DynamicCrtImports.Count -gt 0) {
        throw ("Static CRT audit failed; unexpected dynamic CRT imports: {0}" -f ($DynamicCrtImports -join ", "))
    }

    if (Test-Path -LiteralPath $OutputRoot -PathType Container) {
        $Unexpected = @(
            Get-ChildItem -LiteralPath $OutputRoot -Force |
                Where-Object { $_.Name -ne "Vkit.exe" }
        )
        if ($Unexpected.Count -gt 0) {
            throw ("Refusing to publish into a non-empty output directory. Unexpected item(s): {0}" -f (($Unexpected | Select-Object -ExpandProperty FullName) -join ", "))
        }
    } else {
        New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
    }

    try {
        Move-Item -LiteralPath $StagedExe -Destination $Destination -Force
    } catch {
        if (-not (Test-Path -LiteralPath $StagedExe -PathType Leaf)) {
            throw
        }
        try {
            New-Item -ItemType Directory -Path $PendingRoot -Force | Out-Null
            Copy-Item -LiteralPath $StagedExe -Destination $PendingDestination -Force
            $PendingHash = (Get-FileHash -LiteralPath $PendingDestination -Algorithm SHA256).Hash
            $StagedHash = (Get-FileHash -LiteralPath $StagedExe -Algorithm SHA256).Hash
            if ($PendingHash -ne $StagedHash) {
                throw "Pending release SHA-256 verification failed."
            }
            $Destination = $PendingDestination
            $PublishState = "pending"
            Write-Warning "Vkit.exe is locked. The validated release was preserved at '$PendingDestination'. Close the app and build again to publish it."
        } catch {
            $PreserveStageOnFailure = $true
            throw "Publishing failed and the validated staging artifact was preserved at '$StagedExe': $($_.Exception.Message)"
        }
    }
}
finally {
    if ((Test-Path -LiteralPath $StageRoot) -and -not $PreserveStageOnFailure) {
        Remove-Item -LiteralPath $StageRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$PublishedItems = if ($PublishState -eq "published") {
    @(Get-ChildItem -LiteralPath $OutputRoot -Force)
} else {
    @()
}
if ($PublishState -eq "published") {
    $ExeItems = @($PublishedItems | Where-Object { $_.Name -eq "Vkit.exe" -and -not $_.PSIsContainer })
    if ($PublishedItems.Count -ne 1 -or $ExeItems.Count -ne 1) {
        throw ("Native distribution must hold Vkit.exe and nothing else; found: {0}" -f
            (($PublishedItems | ForEach-Object { $_.Name }) -join ", "))
    }
}

$Item = Get-Item -LiteralPath $Destination
$Hash = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
$Signature = Get-AuthenticodeSignature -LiteralPath $Destination

[pscustomobject]@{
    path = $Destination
    build_session = $BuildSession
    tests_run = -not $SkipTests
    unsigned_source_bytes = $SourceItem.Length
    bytes = $Item.Length
    signature_overhead_bytes = $Item.Length - $SourceItem.Length
    mebibytes = [math]::Round($Item.Length / 1MB, 3)
    sha256 = $Hash
    profile = $Profile
    target_triple = $TargetTriple
    cargo_target_dir = $CargoTargetDir
    maximum_mib = $MaximumMiB
    distribution_item_count = $PublishedItems.Count
    publish_state = $PublishState
    published_to_standard_path = $PublishState -eq "published"
    pending_path = if ($PublishState -eq "pending") { $PendingDestination } else { $null }
    destination_locked_before_build = $DestinationLockedBeforeBuild
    external_runtime_file_count = 0
    static_crt = $true
    imported_dlls = $Imports
    non_windows_imports = $ExternalImports
    signing_mode = if ($UseLocalSigning) { "local-ephemeral" } else { "unsigned" }
    authenticode_status = $Signature.Status.ToString()
    signer_subject = if ($Signature.SignerCertificate) { $Signature.SignerCertificate.Subject } else { $null }
    publisher_trusted = $Signature.Status -eq [System.Management.Automation.SignatureStatus]::Valid
    python_bundled = $false
    blender_bundled = $false
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $ReceiptDir "native-build.json") -Encoding utf8

Remove-ExpiredCargoDiagnostics -Directory $ReceiptDir -KeepSessions $DiagnosticSessionRetention

if ($PublishState -eq "published") {
    Write-Host ("Native Vkit: {0} ({1:N2} MiB)" -f $Destination, ($Item.Length / 1MB)) -ForegroundColor Green
} else {
    Write-Host ("Validated pending Vkit: {0} ({1:N2} MiB)" -f $Destination, ($Item.Length / 1MB)) -ForegroundColor Yellow
}
Write-Host "SHA-256: $Hash"
Write-Host ("Imported DLLs: {0}" -f ($(if ($Imports.Count) { $Imports -join ", " } else { "audit unavailable" })))
