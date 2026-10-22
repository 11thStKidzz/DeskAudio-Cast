param([Parameter(Mandatory)][string]$ReleaseDirectory,[string]$Makensis,[string]$MakeAppx,[string]$MakePri)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$runtimeLock=Get-Content -LiteralPath (Join-Path $repo 'packaging/windows/runtime-lock.json') -Raw | ConvertFrom-Json
$runtimeMaterials=& (Join-Path $repo 'tools/fetch_windows_runtimes.ps1')
$runtimeDirectory=$runtimeMaterials.runtime_directory
$vcRedist=$runtimeMaterials.vc_redist
$release=[IO.Path]::GetFullPath($ReleaseDirectory)
$version=(Get-Content -LiteralPath (Join-Path $repo 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
if($version -notmatch '^\d+\.\d+\.\d+$'){throw 'A three-part release version is required'}
$exe=Join-Path $release 'conexion-airplay.exe'
if(-not(Test-Path -LiteralPath $exe)){throw 'Verified Release EXE required'}
if((Get-Item -LiteralPath $exe).VersionInfo.FileVersion -notin @($version,($version+'.0'))){throw 'Release EXE version differs from package configuration'}
if(-not $Makensis){$Makensis=Join-Path $repo 'local-tools/nsis-3.11/nsis-3.11/Bin/makensis.exe'}
if(-not $MakeAppx){$MakeAppx='C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/makeappx.exe'}
if(-not $MakePri){$MakePri=Join-Path (Split-Path $MakeAppx -Parent) 'makepri.exe'}
foreach($tool in @($Makensis,$MakeAppx,$MakePri)){if(-not(Test-Path -LiteralPath $tool)){throw 'Existing official packaging tool required; no automatic installation'}}
[xml]$manifest=Get-Content -LiteralPath (Join-Path $repo 'packaging/windows/AppxManifest.xml') -Raw
$identity=$manifest.Package.Identity
if($identity.Name -cne '11thStKidzz.DeskAudioCast' -or $identity.Publisher -cne 'CN=F8685FFB-73E3-4962-83BD-CEE132D1ADF6' -or $manifest.Package.Properties.PublisherDisplayName -cne '11thStKidzz'){throw 'Partner Center identity mismatch'}
$storeVersion=[version]$identity.Version
if($storeVersion.Major -lt 1 -or $storeVersion.Revision -ne 0){throw 'Store version requires a nonzero first field and zero fourth field'}
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
$out=Join-Path $repo ('local-packages/'+$stamp)
$payload=Join-Path $out 'payload'
$null=New-Item -ItemType Directory -Path $payload
Copy-Item -LiteralPath $exe -Destination (Join-Path $payload 'DeskAudioCast.exe')
# Identical fixed runtime layout for the custom MSIX and custom NSIS. Merely
# setting Tauri's NSIS bundler mode would not copy files into our MSIX.
Copy-Item -LiteralPath $runtimeDirectory -Destination (Join-Path $payload 'WebView2Fixed') -Recurse
$registry=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.cargo/registry/src/index.crates.io-1949cf8c6b5b557f'
$loader=Join-Path $registry 'webview2-com-sys-0.38.2/x64/WebView2Loader.dll'
# Prefer the corresponding source snapshot's vendored file, usable with an empty Cargo home.
$vendored=Join-Path $repo 'third-party/rust/webview2-com-sys-0.38.2/x64/WebView2Loader.dll'
if(Test-Path -LiteralPath $vendored){$loader=$vendored}
if((Get-FileHash -LiteralPath $loader -Algorithm SHA256).Hash.ToLowerInvariant() -ne '8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c'){throw 'Loader differs from the reviewed Microsoft SDK package'}
Copy-Item -LiteralPath $loader -Destination $payload
Copy-Item -LiteralPath (Join-Path $repo 'legal') -Destination (Join-Path $payload 'legal') -Recurse
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE') -Destination (Join-Path $payload 'LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $repo 'MODIFICATIONS.md') -Destination $payload
Copy-Item -LiteralPath (Join-Path $repo 'vendor/airplay2-gpl3/MODIFICATIONS.md') -Destination (Join-Path $payload 'PROTOCOL_MODIFICATIONS.md')
Copy-Item -LiteralPath (Join-Path $repo 'packaging/windows/使用与安装说明.txt') -Destination (Join-Path $payload 'INSTALLATION.txt')
# Generate explicit install/remove lists. No recursive delete of an install directory.
$install=@();$uninstall=@()
$inventory=foreach($file in Get-ChildItem -LiteralPath $payload -Recurse -File){
    $relative=[IO.Path]::GetRelativePath($payload,$file.FullName)
    if($relative.Contains('..') -or $relative.Contains('$') -or $relative.Contains('"')){throw 'Unsafe payload path'}
    $dir=[IO.Path]::GetDirectoryName($relative)
    $install+='SetOutPath "$INSTDIR'+$(if($dir){'\'+$dir}else{''})+'"'
    $install+='File "${PAYLOAD}\'+$relative+'"'
    $uninstall+='Delete "$INSTDIR\'+$relative+'"'
    @{path=$relative.Replace('\','/');bytes=$file.Length;sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
}
Get-ChildItem -LiteralPath $payload -Recurse -Directory | Sort-Object {$_.FullName.Length} -Descending | ForEach-Object {$uninstall+='RMDir "$INSTDIR\'+[IO.Path]::GetRelativePath($payload,$_.FullName)+'"'}
$installFile=Join-Path $out 'install-files.nsh';$uninstallFile=Join-Path $out 'uninstall-files.nsh'
$install | Set-Content -LiteralPath $installFile -Encoding utf8
$uninstall | Set-Content -LiteralPath $uninstallFile -Encoding utf8
$installer=Join-Path $out ('DeskAudioCast-'+$version+'-x64-setup.exe')
$icon=Join-Path $repo 'src-tauri/icons/desk-audio-cast/icon.ico'
& $Makensis '/V3' '/INPUTCHARSET' 'UTF8' ('/DAPP_VERSION='+$version) ('/DVC_REDIST='+$vcRedist) ('/DVC_MIN_VERSION='+$runtimeLock.vc_redist.minimum_version) ('/DPAYLOAD='+$payload) ('/DOUTPUT='+$installer) ('/DICON='+$icon) ('/DINSTALL_FILES='+$installFile) ('/DUNINSTALL_FILES='+$uninstallFile) (Join-Path $repo 'packaging/windows/installer.nsi') *> (Join-Path $out 'nsis-build.log')
$nsisCode=$LASTEXITCODE
$result=[ordered]@{version=$version;target='windows-x64';public_distribution_authorized=$false;payload=@($inventory);nsis_exit_code=$nsisCode;signed=$false;installed='not_yet_tested';runtime_installation='app-local fixed WebView2; NSIS VC++ official offline installer only with interactive consent and version recheck';webview2_version=$runtimeLock.webview2.version;vc_redist_version=$runtimeLock.vc_redist.version;vc_redist_sha256=$runtimeLock.vc_redist.sha256;msix='not_yet_built'}
$result.store_identity=@{name=[string]$identity.Name;publisher=[string]$identity.Publisher;publisher_display_name=[string]$manifest.Package.Properties.PublisherDisplayName;version=[string]$identity.Version}
if($nsisCode -eq 0){$result.installer_sha256=(Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()}
$result | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $out 'PACKAGE_RECORD.json') -Encoding utf8
if($nsisCode -ne 0){throw ('NSIS failed; see '+$out)}
Copy-Item -LiteralPath (Join-Path $repo 'packaging/windows/AppxManifest.xml') -Destination $payload
Add-Type -AssemblyName System.Drawing
$assets=Join-Path $payload 'Assets';$null=New-Item -ItemType Directory -Path $assets
$image=[Drawing.Image]::FromFile((Join-Path $repo 'src-tauri/icons/desk-audio-cast/256x256.png'))
try {
    foreach($entry in @(@('StoreLogo',50),@('Square150x150Logo',150),@('Square44x44Logo',44))){
        $bitmap=[Drawing.Bitmap]::new($entry[1],$entry[1]);$graphics=[Drawing.Graphics]::FromImage($bitmap)
        try {$graphics.InterpolationMode=[Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic;$graphics.DrawImage($image,0,0,$entry[1],$entry[1]);$bitmap.Save((Join-Path $assets ($entry[0]+'.png')),[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
    }
} finally {$image.Dispose()}
Copy-Item -LiteralPath (Join-Path $repo 'packaging/windows/Strings') -Destination $payload -Recurse
$priConfig=Join-Path $out 'priconfig.xml'
& $MakePri createconfig /cf $priConfig /dq en-US /pv 10.0.0 /o *> (Join-Path $out 'pri-config.log')
if($LASTEXITCODE -ne 0){throw 'MakePri configuration failed'}
# Keep every language in this one x64 package, not separate language resource packs.
[xml]$priSettings=Get-Content -LiteralPath $priConfig -Raw
foreach($node in @($priSettings.SelectNodes('//packaging'))){$null=$node.ParentNode.RemoveChild($node)}
$priSettings.Save($priConfig)
& $MakePri new /pr $payload /cf $priConfig /mn (Join-Path $payload 'AppxManifest.xml') /of (Join-Path $payload 'resources.pri') /o *> (Join-Path $out 'pri-build.log')
if($LASTEXITCODE -ne 0){throw 'MakePri resource compilation failed'}
$priDump=Join-Path $out 'resources-dump.xml'
& $MakePri dump /if (Join-Path $payload 'resources.pri') /of $priDump /dt detailed /o *> (Join-Path $out 'pri-dump.log')
if($LASTEXITCODE -ne 0){throw 'MakePri resource verification failed'}
$dump=Get-Content -LiteralPath $priDump -Raw
foreach($expected in @('DeskAudio Cast','桌面声渡','en-US','zh-Hans','11thStKidzz.DeskAudioCast')){if(-not $dump.Contains($expected)){throw ('Missing compiled resource: '+$expected)}}
foreach($file in Get-ChildItem -LiteralPath $payload -Recurse -File){if([IO.Path]::GetRelativePath($payload,$file.FullName) -match '[^\x00-\x7F]'){throw 'Store payload filenames must be ASCII'}}
$result.resource_compilation='passed_english_default_simplified_chinese_names_verified_in_pri_dump'
$msix=Join-Path $out ('DeskAudioCast-'+$storeVersion+'-x64-store-upload.msix')
& $MakeAppx pack /d $payload /p $msix /o *> (Join-Path $out 'msix-build.log')
$result.msix_exit_code=$LASTEXITCODE
if($LASTEXITCODE -eq 0){$result.msix='unsigned_store_upload_candidate_not_trusted_for_direct_install_not_submitted';$result.msix_sha256=(Get-FileHash -LiteralPath $msix -Algorithm SHA256).Hash.ToLowerInvariant()}
$result | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $out 'PACKAGE_RECORD.json') -Encoding utf8
Write-Output ('Packages: '+$out)
if($result.msix_exit_code -ne 0){throw 'MakeAppx packaging failed'}
