$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifestPath=Join-Path $repo 'SOURCE_PACKAGE_MANIFEST.json'
if(-not(Test-Path -LiteralPath $manifestPath)){throw 'Run this from an extracted source package, not the development checkout.'}
$manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$profilePath=[Environment]::GetFolderPath('UserProfile')
$python=Join-Path $profilePath 'AppData/Local/Programs/Python/Python312/python.exe'
if(-not(Test-Path -LiteralPath $python)){$python=(Get-Command python -ErrorAction Stop).Source}
& $python -B (Join-Path $PSScriptRoot 'verify_snapshot.py') --root $repo
if($LASTEXITCODE -ne 0){throw 'Extracted snapshot verification failed'}
$record=Join-Path $repo ('local-validation/archive-build-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
$null=New-Item -ItemType Directory -Path $record
$profilePath=[Environment]::GetFolderPath('UserProfile')
$python=Join-Path $profilePath 'AppData/Local/Programs/Python/Python312/python.exe'
if(-not(Test-Path -LiteralPath $python)){$python=(Get-Command python -ErrorAction Stop).Source}
$identity=(& $python -B (Join-Path $PSScriptRoot 'build_identity.py') --root $repo).Trim()
if($identity -ne $manifest.source_fingerprint){throw 'Product identity mismatch'}
$env:CARGO_HOME=Join-Path $record 'empty-cargo-home'
$env:CARGO_TARGET_DIR=Join-Path $repo 'target/archive-check'
if(Test-Path -LiteralPath $env:CARGO_TARGET_DIR){throw 'Build target already exists; refusing to call an incremental build clean.'}
$env:CARGO_BUILD_JOBS='4'
$env:CARGO_ENCODED_RUSTFLAGS=@(('--remap-path-prefix='+$profilePath+'=/local'),('--remap-path-prefix='+$profilePath.Replace('\','/')+'=/local'),('--remap-path-prefix='+$repo+'=/src'),('--remap-path-prefix='+$repo.Replace('\','/')+'=/src')) -join [char]31
$env:DESKAUDIO_BUILD_REVISION=$manifest.application_baseline
$env:DESKAUDIO_SOURCE_FINGERPRINT=$identity
$env:DESKAUDIO_LINK_MAP=Join-Path $env:CARGO_TARGET_DIR 'release/desk-audio-link.map'
if(-not $env:DESKAUDIO_VSDEVCMD){$env:DESKAUDIO_VSDEVCMD='C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat'}
$result=[ordered]@{source_fingerprint=$identity;empty_cargo_home=$true;new_target=$true;fresh_windows_os=$false;offline=$true;uploads=$false;results=@()}
$npm=(Get-Command npm.cmd -ErrorAction Stop).Source
Push-Location $repo
try {
    foreach($stage in @('ui-test','ui-build','python-tests','rust-tests','release')){
        $log=Join-Path $record ($stage+'.log')
        # Windows PowerShell 5 represents native stderr as ErrorRecord objects.
        # unittest/cargo normally write progress there; preserve it in the log
        # and use the real native exit code, never the presence of stderr.
        $savedPreference=$ErrorActionPreference
        $LASTEXITCODE=$null
        try {
        $ErrorActionPreference='Continue'
        switch($stage){
            'ui-test' {& $npm --prefix ui test *> $log}
            'ui-build' {& $npm --prefix ui run build *> $log}
            'python-tests' {& $python -B -m unittest discover -s tools/tests -v *> $log}
            'rust-tests' {& cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" test --workspace --all-targets --locked --offline') *> $log}
            'release' {& cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" build -p conexion-airplay --release --locked --offline') *> $log}
        }
        } finally { $ErrorActionPreference=$savedPreference }
        if($null -eq $LASTEXITCODE -or -not(Test-Path -LiteralPath $log -PathType Leaf)){throw 'Native command did not run or its log could not be written'}
        $code=$LASTEXITCODE;$result.results+=@{stage=$stage;exit_code=$code}
        $result | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $record 'record.json') -Encoding utf8
        Write-Output ($stage+': exit '+$code)
        if($code -ne 0){exit $code}
    }
    & (Join-Path $profilePath '.cargo/bin/rustc.exe') -vV | Set-Content (Join-Path $record 'rustc-version.txt')
    & node --version | Set-Content (Join-Path $record 'node-version.txt')
} finally {Pop-Location}
