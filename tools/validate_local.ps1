param([ValidateSet('lock','rust','protocol','release','inventory','ui','python','deployment','all')][string]$Stage='all',[string]$TargetDirectory)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
$record=Join-Path $repo ('local-validation\'+$stamp+'-'+$Stage)
$null=New-Item -ItemType Directory -Path $record
Push-Location $repo
try {
    $env:CARGO_TARGET_DIR=if($TargetDirectory){[IO.Path]::GetFullPath($TargetDirectory)}else{Join-Path $repo 'target\readiness'}
    $env:DESKAUDIO_LINK_MAP=Join-Path $env:CARGO_TARGET_DIR 'release/desk-audio-link.map'
    # Apply to Rust sources, macro locations and panic messages, not just external PDBs.
    $taskProfile=[Environment]::GetFolderPath('UserProfile')
    $env:CARGO_ENCODED_RUSTFLAGS=@(('--remap-path-prefix='+$taskProfile+'=/local'),('--remap-path-prefix='+$taskProfile.Replace('\','/')+'=/local'),('--remap-path-prefix='+$repo+'=/src'),('--remap-path-prefix='+$repo.Replace('\','/')+'=/src')) -join [char]31
    $head=(& git rev-parse HEAD).Trim()
    $dirty=(@(& git --no-optional-locks status --porcelain).Count -ne 0)
    $env:DESKAUDIO_BUILD_REVISION=$head+$(if($dirty){'-d'}else{''})
    $python=Join-Path $taskProfile 'AppData\Local\Programs\Python\Python312\python.exe'
    $env:DESKAUDIO_SOURCE_FINGERPRINT=(& $python -B (Join-Path $PSScriptRoot 'build_identity.py') --root $repo --manifest (Join-Path $record 'source-files.sha256')).Trim()
    if($LASTEXITCODE -ne 0){throw 'Source identity failed'}
    $metadata=[ordered]@{stage=$Stage; utc=(Get-Date).ToUniversalTime().ToString('o'); head=$head; dirty=$dirty; source_fingerprint=$env:DESKAUDIO_SOURCE_FINGERPRINT; uploads=$false; hardware_tests='not_run'; clean_environment=$false; results=@()}
    $vs='C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat'
    if($env:DESKAUDIO_VSDEVCMD){$vs=$env:DESKAUDIO_VSDEVCMD}
    $env:DESKAUDIO_VSDEVCMD=$vs
    if(-not(Test-Path -LiteralPath $vs)){throw 'MSVC build environment missing; no tools will be installed automatically.'}
    $cargo=Join-Path $taskProfile '.cargo\bin\cargo.exe'
    $commands=@{
        lock=@('metadata --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc')
        rust=@('test --workspace --all-targets --locked --offline')
        protocol=@('test --manifest-path vendor/airplay2-gpl3/Cargo.toml -p airplay-audio --lib --offline')
        release=@('build -p conexion-airplay --release --locked --offline')
        deployment=@('test -p conexion-airplay --lib runtime_prerequisites --locked --offline','test -p conexion-airplay --lib legal::tests --locked --offline')
        inventory=@('metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc','tree --locked --offline -p conexion-airplay --target x86_64-pc-windows-msvc --edges normal,build')
    }
    if($Stage -in @('ui','python')) {
        if($Stage -eq 'ui') {
            $npm='C:\Program Files\nodejs\npm.cmd'
            & $npm --prefix ui test *> (Join-Path $record 'ui-test.log')
            $testCode=$LASTEXITCODE
            & $npm --prefix ui run build *> (Join-Path $record 'ui-build.log')
            $buildCode=$LASTEXITCODE
            $metadata.results=@(@{step='ui-test';exit_code=$testCode},@{step='ui-build';exit_code=$buildCode})
            $code=if($testCode -ne 0){$testCode}else{$buildCode}
        } else {
            $python=Join-Path $taskProfile 'AppData\Local\Programs\Python\Python312\python.exe'
            & $python -B -m unittest discover -s tools/tests -v *> (Join-Path $record 'python-test.log')
            $code=$LASTEXITCODE
            $metadata.results=@(@{step='python-tests';exit_code=$code})
        }
        $metadata | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $record 'record.json') -Encoding utf8
        Write-Output ($Stage+': exit '+$code+'; '+$record)
        exit $code
    }
    $stages=if($Stage -eq 'all'){@('lock','rust','release','inventory')}else{@($Stage)}
    foreach($name in $stages) {
        $index=0
        foreach($arguments in $commands[$name]) {
            $index++
            $log=Join-Path $record ($name+'-'+$index+'.log')
            & cmd.exe /d /c ('call "'+(Join-Path $PSScriptRoot 'msvc_cargo.cmd')+'" '+$arguments) *> $log
            $code=$LASTEXITCODE
            $metadata.results+=@{step=$name; command=('cargo '+$arguments); exit_code=$code; log=[IO.Path]::GetFileName($log)}
            Write-Output ($name+': exit '+$code+'; '+$log)
            if($code -ne 0){ break }
        }
        if($code -ne 0){break}
    }
    & $cargo --version | Set-Content -LiteralPath (Join-Path $record 'cargo-version.txt')
    & (Join-Path $taskProfile '.cargo\bin\rustc.exe') -vV | Set-Content -LiteralPath (Join-Path $record 'rustc-version.txt')
    $metadata | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $record 'record.json') -Encoding utf8
    exit $code
} finally { Pop-Location }
