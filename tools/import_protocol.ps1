# Local source import only. Never modifies the Cargo cache or downloads anything.
param([Parameter(Mandatory=$true)][string]$Source)
$ErrorActionPreference='Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination = Join-Path $repo 'vendor\airplay2-rs'
if (Test-Path -LiteralPath $destination) { throw 'Protocol source already exists; refusing to overwrite.' }
if ((& git -C $Source rev-parse HEAD) -ne 'dc66e2b3df1ad87a34fd9c859ec452bd787154e1') { throw 'Unexpected upstream revision.' }
$null = New-Item -ItemType Directory -Path $destination
Copy-Item -LiteralPath (Join-Path $Source 'crates') -Destination $destination -Recurse
foreach($name in @('Cargo.toml','LICENSE','README.md')) { Copy-Item -LiteralPath (Join-Path $Source $name) -Destination $destination }
$files=Get-ChildItem -LiteralPath $destination -File -Recurse | ForEach-Object {
    [ordered]@{path=[IO.Path]::GetRelativePath($destination,$_.FullName).Replace('\','/'); sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
}
@{source='https://github.com/Pabldi08/airplay2-rs'; revision='dc66e2b3df1ad87a34fd9c859ec452bd787154e1'; warning='Original imported-file hashes before local patches; licensing unresolved; no distribution authorization inferred.'; files=@($files)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'UPSTREAM_MANIFEST.json') -Encoding utf8
Write-Output ('Imported source files: '+@($files).Count)
