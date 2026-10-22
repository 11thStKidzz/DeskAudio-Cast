param([string]$ReleaseDirectory)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(-not $ReleaseDirectory){$ReleaseDirectory=Join-Path $repo 'target\readiness\release'}
$evidence=Join-Path $repo ('local-validation\'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-artifact-review')
$null=New-Item -ItemType Directory -Path $evidence
$files=@('conexion-airplay.exe','conexion_airplay_lib.dll') | ForEach-Object {Join-Path $ReleaseDirectory $_}
foreach($file in $files){if(-not(Test-Path -LiteralPath $file)){throw 'Actual release binary missing.'}}
$python=Join-Path ([Environment]::GetFolderPath('UserProfile')) 'AppData\Local\Programs\Python\Python312\python.exe'
& $python -B (Join-Path $PSScriptRoot 'privacy_scan.py') --repo $repo --binary $files[0] --binary $files[1] --output (Join-Path $evidence 'privacy-scan.json')
if($LASTEXITCODE -ne 0){throw 'Privacy scan did not complete.'}
$map=Join-Path $ReleaseDirectory 'desk-audio-link.map'
if(-not(Test-Path -LiteralPath $map)){throw 'Final EXE linker map missing.'}
$fdk=@(Select-String -LiteralPath $map -Pattern 'aacEncOpen|aacEncEncode|aacEncoder_SetParam|fdk_aac|fdk-aac')
$alac=@(Select-String -LiteralPath $map -Pattern 'alac_encoder')
$dumpbin='C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\MSVC\14.51.36231\bin\Hostx64\x64\dumpbin.exe'
$artifacts=foreach($file in $files){
    $item=Get-Item -LiteralPath $file
    if(Test-Path -LiteralPath $dumpbin){
        & $dumpbin /headers $file | Set-Content -LiteralPath (Join-Path $evidence ($item.Name+'.headers.txt'))
        & $dumpbin /dependents $file | Set-Content -LiteralPath (Join-Path $evidence ($item.Name+'.imports.txt'))
    }
    [ordered]@{name=$item.Name; bytes=$item.Length; sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant(); file_version=$item.VersionInfo.FileVersion; product_name=$item.VersionInfo.ProductName}
}
$scan=Get-Content -LiteralPath (Join-Path $evidence 'privacy-scan.json') -Raw | ConvertFrom-Json
$report=[ordered]@{
    review='actual Release EXE/DLL plus final EXE linker map; not an installer certification'
    artifacts=@($artifacts); fdk_map_matches=$fdk.Count; alac_map_matches=$alac.Count
    personal_path_or_secret_pattern_findings=@($scan.binaries | ForEach-Object {$_.findings}).Count
    fdk_binary_marker_findings=@($scan.binaries | ForEach-Object {$_.fdk_markers}).Count
    source_files_flagged=@($scan.source).Count
    warnings=@('Pattern/symbol checks are not universal proofs','Old private/debug artifacts are not release inputs; owned log cleanup is recorded separately','This scan does not perform signing, installation, HomePod or clean-Windows tests','Public distribution remains gated by source delivery, external rights and Store requirements; see RELEASE_GATES.json')
}
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidence 'review.json') -Encoding utf8
Write-Output ('Review recorded at '+$evidence)
if($fdk.Count -ne 0 -or $alac.Count -eq 0 -or $report.personal_path_or_secret_pattern_findings -ne 0 -or $report.fdk_binary_marker_findings -ne 0){exit 1}
