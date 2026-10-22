# One-time, explicitly user-authorized cleanup. Does not stop processes or recurse.
param([switch]$IncludeCurrent)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$logRoot=[IO.Path]::GetFullPath((Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'ConexionAirPlay/logs'))
$record=Join-Path $repo ('local-validation/'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-log-cleanup')
$null=New-Item -ItemType Directory -Path $record
$results=@()
if(Test-Path -LiteralPath $logRoot){
    $folder=Get-Item -LiteralPath $logRoot
    if($folder.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Refusing reparse-point log directory'}
    foreach($file in Get-ChildItem -LiteralPath $logRoot -File){
        if($file.Name -notmatch '^app\.log\.\d{4}-\d{2}-\d{2}$'){continue}
        if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){continue}
        $path=[IO.Path]::GetFullPath($file.FullName)
        if([IO.Path]::GetDirectoryName($path) -ne $logRoot){throw 'Out-of-scope target'}
        # Keep today's potentially active session intact, even if temporarily unlocked.
        if(-not $IncludeCurrent -and $file.Name -eq ('app.log.'+(Get-Date -Format 'yyyy-MM-dd'))){
            $results+=@{name=$file.Name;result='preserved_current_session'};continue
        }
        try {
            $stream=[IO.FileStream]::new($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None,4096,[IO.FileOptions]::DeleteOnClose)
            try { $bytes=$stream.Length } finally {$stream.Dispose()}
            $results+=@{name=$file.Name;bytes=$bytes;result='removed_exclusively_opened_closed_old_app_log'}
        } catch [IO.IOException] { $results+=@{name=$file.Name;result='preserved_locked_or_unavailable'} }
    }
}
@{scope='%APPDATA%/ConexionAirPlay/logs';recursive=$false;stopped_processes=$false;results=$results;recovery='Deleted logs are not recoverable through this script; source and test evidence untouched'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $record 'record.json') -Encoding utf8
Write-Output ($results | ConvertTo-Json -Depth 4)
Write-Output ('Record: '+$record)
