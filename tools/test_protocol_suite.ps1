$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$record=Join-Path $repo ('local-validation/'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-protocol-suite')
$null=New-Item -ItemType Directory -Path $record
Copy-Item -LiteralPath (Join-Path $repo 'vendor/airplay2-gpl3') -Destination (Join-Path $record 'source') -Recurse
$env:CARGO_TARGET_DIR=Join-Path $repo 'target/protocol-suite'
$env:DESKAUDIO_VSDEVCMD='C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat'
Push-Location $record
try {
    # Isolated lock may update; product Cargo.lock and source remain untouched.
    & cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" test --manifest-path source/Cargo.toml --workspace --lib --offline') *> (Join-Path $record 'tests.log')
    $code=$LASTEXITCODE
    [ordered]@{package='all protocol workspace libraries';source_revision='f824007798f71605307384829800273a4dbbcee2-with-recorded-adaptations';exit_code=$code;scope='upstream library unit tests in isolated source copy';homepod_tests=$false;uploads=$false} | ConvertTo-Json | Set-Content (Join-Path $record 'record.json')
    Write-Output ('Protocol suite exit '+$code+'; '+$record)
    exit $code
} finally {Pop-Location}
