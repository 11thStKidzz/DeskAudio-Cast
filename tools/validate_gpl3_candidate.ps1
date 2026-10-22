param([ValidateSet('check','rust','protocol','release','inventory')][string]$Stage='check')
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$candidate=Join-Path $repo 'local-validation/gpl3-candidate-20261021'
$record=Join-Path $repo ('local-validation/'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-gpl3-'+$Stage)
$null=New-Item -ItemType Directory -Path $record
$taskProfile=[Environment]::GetFolderPath('UserProfile')
$env:CARGO_TARGET_DIR=Join-Path $repo 'target/gpl3-candidate'
$env:CARGO_ENCODED_RUSTFLAGS=@(('--remap-path-prefix='+$taskProfile+'=/local'),('--remap-path-prefix='+$taskProfile.Replace('\','/')+'=/local')) -join [char]31
$env:DESKAUDIO_BUILD_REVISION=(& git -C $repo rev-parse HEAD).Trim()+'-d'
$env:DESKAUDIO_VSDEVCMD='C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat'
$paths=Get-ChildItem -LiteralPath $candidate -Recurse -File | Where-Object { $_.FullName -notmatch '[\\/](dist|target|node_modules|\.git|gen)[\\/]' -and $_.Extension -ne '.tar' } | Sort-Object FullName
$lines=@($paths | ForEach-Object {[IO.Path]::GetRelativePath($candidate,$_.FullName).Replace('\','/')+':'+(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()})
$sha=[Security.Cryptography.SHA256]::Create()
$env:DESKAUDIO_SOURCE_FINGERPRINT=[Convert]::ToHexString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n")))).ToLowerInvariant()
$sha.Dispose()
$lines | Set-Content (Join-Path $record 'source-files.sha256') -Encoding utf8
$arguments=@{
    check='check --workspace --all-targets --offline'
    rust='test --workspace --all-targets --locked --offline'
    protocol='test --manifest-path vendor/airplay2-rs/Cargo.toml -p airplay-audio --lib --locked --offline'
    release='rustc -p conexion-airplay --bin conexion-airplay --release --locked --offline -- -C link-arg=/MAP:'+(Join-Path $repo 'target/gpl3-candidate/release/desk-audio-link.map')
    inventory='metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc --locked --offline'
}[$Stage]
Push-Location $candidate
try {
    if ($Stage -eq 'release') {
        & cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" build -p conexion-airplay --release --locked --offline') *> (Join-Path $record 'release-build.log')
        if ($LASTEXITCODE -ne 0) {
            $code=$LASTEXITCODE
            [ordered]@{stage=$Stage;exit_code=$code;uploads=$false;hardware_tests='not_run'} | ConvertTo-Json | Set-Content (Join-Path $record 'record.json')
            Write-Output ('Candidate release failed: '+$record)
            exit $code
        }
    }
    & cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" '+$arguments) *> (Join-Path $record 'cargo.log')
    $code=$LASTEXITCODE
    [ordered]@{stage=$Stage;utc=(Get-Date).ToUniversalTime().ToString('o');application_baseline=$env:DESKAUDIO_BUILD_REVISION;upstream_revision='f824007798f71605307384829800273a4dbbcee2';source_fingerprint=$env:DESKAUDIO_SOURCE_FINGERPRINT;exit_code=$code;uploads=$false;homepod_tests='not_run';production_backend_changed=$false} | ConvertTo-Json | Set-Content (Join-Path $record 'record.json') -Encoding utf8
    Write-Output ('Candidate '+$Stage+': exit '+$code+'; '+$record)
    exit $code
} finally {Pop-Location}
