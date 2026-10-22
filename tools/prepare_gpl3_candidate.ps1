param([string]$Destination = 'local-validation/gpl3-candidate-20261021')
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$candidate = [IO.Path]::GetFullPath((Join-Path $repo $Destination))
if (-not $candidate.StartsWith($repo + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Candidate must be inside workspace.' }
if (Test-Path -LiteralPath $candidate) { throw 'Candidate already exists; refusing to overwrite.' }
$revision = 'f824007798f71605307384829800273a4dbbcee2'
$history = Join-Path $repo 'local-validation/f1-20261021/upstream.git'
$null = New-Item -ItemType Directory -Path $candidate
Push-Location $repo
try {
    # App GPLv3 sources only. Never copy the old protocol fork, caches, logs or identities.
    $paths = @(& git -c core.quotepath=false ls-files --cached --others --exclude-standard) | Sort-Object -Unique
    $paths = $paths | Where-Object { $_ -match '^(Cargo\.(toml|lock)$|LICENSE$|crates/|src-tauri/|ui/|legal/)' }
    foreach ($p in $paths) {
        if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { continue }
        $dest = Join-Path $candidate $p
        $null = New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent)
        Copy-Item -LiteralPath $p -Destination $dest
    }
    # Existing locally built web assets; no dependency installation or network access.
    if (Test-Path 'ui/dist') { Copy-Item -LiteralPath 'ui/dist' -Destination (Join-Path $candidate 'ui/dist') -Recurse }
    $protocol = Join-Path $candidate 'vendor/airplay2-rs'
    $null = New-Item -ItemType Directory -Path $protocol
    $archive = Join-Path $candidate 'upstream-source.tar'
    & git --git-dir=$history archive --format=tar --output=$archive $revision LICENSE README.md Cargo.toml Cargo.lock crates/airplay-core crates/airplay-discovery crates/airplay-crypto crates/airplay-pairing crates/airplay-rtsp crates/airplay-audio crates/airplay-timing crates/airplay-client crates/airplay-resampler
    if ($LASTEXITCODE -ne 0) { throw 'Pinned GPLv3 source export failed.' }
    & tar -xf $archive -C $protocol
    if ($LASTEXITCODE -ne 0) { throw 'Source extraction failed.' }
    $manifest = Get-ChildItem -LiteralPath $protocol -Recurse -File | ForEach-Object {
        [ordered]@{path=[IO.Path]::GetRelativePath($protocol,$_.FullName).Replace('\','/');sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
    }
    [ordered]@{repository='https://github.com/lmcgartland/airplay2-rs';revision=$revision;license='GPL-3.0-or-later';original_files=@($manifest);old_fork_copied=$false;public_distribution_authorized=$false} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $candidate 'UPSTREAM_ORIGIN.json') -Encoding utf8
    Write-Output ('Isolated candidate: ' + $candidate)
    Write-Output ('Original upstream files: ' + @($manifest).Count)
} finally { Pop-Location }
