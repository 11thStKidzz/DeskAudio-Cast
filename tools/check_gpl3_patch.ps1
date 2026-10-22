$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$record=Join-Path $repo ('local-validation/'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-gpl3-patch-replay')
$null=New-Item -ItemType Directory -Path $record
$archive=Join-Path $record 'upstream.tar'
$source=Join-Path $record 'source'
$null=New-Item -ItemType Directory -Path $source
Push-Location $repo
try {
    & git -c core.autocrlf=false --git-dir=local-validation/f1-20261021/upstream.git archive --format=tar --output=$archive f824007798f71605307384829800273a4dbbcee2 LICENSE README.md Cargo.toml Cargo.lock crates/airplay-core crates/airplay-discovery crates/airplay-crypto crates/airplay-pairing crates/airplay-rtsp crates/airplay-audio crates/airplay-timing crates/airplay-client crates/airplay-resampler
    if($LASTEXITCODE -ne 0){throw 'Source export failed'}
    & tar -xf $archive -C $source
    if($LASTEXITCODE -ne 0){throw 'Source extraction failed'}
    $relative=[IO.Path]::GetRelativePath($repo,$source).Replace('\','/')
    & git apply --check --directory=$relative docs/license-evidence/gpl3-migration/protocol.patch *> (Join-Path $record 'patch-check.log')
    $code=$LASTEXITCODE
    [ordered]@{upstream='f824007798f71605307384829800273a4dbbcee2';patch_sha256=(Get-FileHash docs/license-evidence/gpl3-migration/protocol.patch -Algorithm SHA256).Hash.ToLowerInvariant();check_exit_code=$code;applied=$false;clean_build=$false;uploads=$false} | ConvertTo-Json | Set-Content (Join-Path $record 'record.json') -Encoding utf8
    Write-Output ('GPLv3 patch application check: exit '+$code+'; '+$record)
    exit $code
} finally {Pop-Location}
