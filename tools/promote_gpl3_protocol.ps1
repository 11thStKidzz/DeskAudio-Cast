$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$candidate=Join-Path $repo 'local-validation/gpl3-candidate-20261021'
$destination=Join-Path $repo 'vendor/airplay2-gpl3'
if(Test-Path -LiteralPath $destination){throw 'Destination exists; refusing to overwrite.'}
$evidence=Get-Content (Join-Path $repo 'docs/license-evidence/gpl3-migration/candidate-source.json') -Raw | ConvertFrom-Json
if($evidence.upstream_revision -ne 'f824007798f71605307384829800273a4dbbcee2' -or $evidence.old_fork_copied){throw 'Unexpected source origin'}
foreach($item in $evidence.candidate_source_files){
    if((Get-FileHash -LiteralPath (Join-Path $candidate $item.path) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.sha256){throw ('Candidate changed: '+$item.path)}
}
$null=New-Item -ItemType Directory -Path $destination
foreach($item in $evidence.protocol_files){
    $source=Join-Path $candidate ('vendor/airplay2-rs/'+$item.path)
    $target=Join-Path $destination $item.path
    $null=New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent)
    Copy-Item -LiteralPath $source -Destination $target
    if((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ne $item.sha256){throw 'Copy hash mismatch'}
}
Copy-Item -LiteralPath (Join-Path $candidate 'UPSTREAM_ORIGIN.json') -Destination (Join-Path $destination 'UPSTREAM_ORIGIN.json')
$test=Join-Path $repo 'crates/airplay-core/tests/gpl3_migration.rs'
if(Test-Path -LiteralPath $test){throw 'Migration regression tests already exist'}
Copy-Item -LiteralPath (Join-Path $candidate 'crates/airplay-core/tests/gpl3_migration.rs') -Destination $test
Write-Output 'Verified GPLv3 source copied to a NEW vendor directory. Old fork and binaries preserved.'
