param([switch]$Download,[string]$CabPath)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$lock=Get-Content -LiteralPath (Join-Path $repo 'packaging/windows/runtime-lock.json') -Raw | ConvertFrom-Json
$base=Join-Path $repo ('local-tools/webview2-fixed-'+$lock.webview2.version)
$cab=Join-Path $base ('Microsoft.WebView2.FixedVersionRuntime.'+$lock.webview2.version+'.x64.cab')
$redist=Join-Path $repo 'local-tools/vc_redist.x64.exe'
if($CabPath -and -not(Test-Path -LiteralPath $cab)){
    if((Get-FileHash -LiteralPath $CabPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $lock.webview2.sha256){throw 'Provided official CAB differs from the reviewed hash'}
    New-Item -ItemType Directory -Path $base -Force | Out-Null
    Copy-Item -LiteralPath $CabPath -Destination $cab
}
foreach($entry in @(@($cab,$lock.webview2),@($redist,$lock.vc_redist))){
    if(-not(Test-Path -LiteralPath $entry[0])){
        if($entry[0] -eq $cab){throw 'Obtain the reviewed official Fixed Runtime directly from Microsoft under its terms; provide -CabPath. The source repository is not a runtime download service.'}
        if(-not $Download){throw 'Official runtime cache missing. Read DEPLOYMENT.md; use -Download only with permission.'}
        New-Item -ItemType Directory -Path (Split-Path $entry[0] -Parent) -Force | Out-Null
        Invoke-WebRequest -Uri $entry[1].url -OutFile $entry[0] -TimeoutSec 600
    }
    if((Get-FileHash -LiteralPath $entry[0] -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry[1].sha256){throw 'Runtime hash differs from reviewed lock; do not silently accept a newer download'}
}
$expanded=Join-Path $base 'expanded'
if(-not(Test-Path -LiteralPath $expanded)){
    New-Item -ItemType Directory -Path $expanded | Out-Null
    & "$env:WINDIR/System32/expand.exe" $cab '-F:*' $expanded | Out-Null
    if($LASTEXITCODE -ne 0){throw 'Runtime CAB extraction failed; preserve incomplete cache for diagnosis'}
}
$runtime=Join-Path $expanded ('Microsoft.WebView2.FixedVersionRuntime.'+$lock.webview2.version+'.x64')
foreach($file in @((Join-Path $runtime 'msedgewebview2.exe'),(Join-Path $runtime 'msedge.dll'),$redist)){
    $signature=Get-AuthenticodeSignature -LiteralPath $file
    if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation'){throw 'Microsoft runtime signature not valid'}
}
if((Get-Item -LiteralPath (Join-Path $runtime 'msedgewebview2.exe')).VersionInfo.FileVersion -cne $lock.webview2.version){throw 'Runtime executable version mismatch'}
$expected=Get-Content -LiteralPath (Join-Path $repo 'packaging/windows/webview2-files.sha256.json') -Raw | ConvertFrom-Json
$actual=@(Get-ChildItem -LiteralPath $runtime -Recurse -File)
if($actual.Count -ne $expected.files.Count){throw 'Runtime file count mismatch'}
foreach($entry in $expected.files){
    $relative=[string]$entry.path
    if([IO.Path]::IsPathRooted($relative) -or $relative.Split('/').Contains('..')){throw 'Unsafe runtime manifest path'}
    $file=Join-Path $runtime $relative
    if((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256){throw 'Fixed Runtime file checksum mismatch'}
}
# No EXE execution, system installation, certificate import or ACL changes here.
[pscustomobject]@{runtime_directory=$runtime;vc_redist=$redist;runtime_version=$lock.webview2.version;verified_files=$actual.Count}
