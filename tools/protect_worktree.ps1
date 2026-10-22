param([string]$Destination)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $Destination) {
    $Destination = Join-Path (Split-Path $repo -Parent) ('private-backups\before-remediation-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
if (Test-Path -LiteralPath $Destination) { throw 'Backup destination already exists; refusing to overwrite.' }
$null = New-Item -ItemType Directory -Path $Destination
Push-Location $repo
try {
    $paths = @(& git -c core.quotepath=false ls-files --cached --others --exclude-standard)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot enumerate worktree.' }
    $manifest = foreach ($relative in $paths) {
        $source = Join-Path $repo $relative
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { continue }
        $target = Join-Path $Destination $relative
        $null = New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent)
        Copy-Item -LiteralPath $source -Destination $target
        $originalHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
        if ($originalHash -ne (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash) { throw 'Backup hash mismatch.' }
        [ordered]@{path=$relative; sha256=$originalHash}
    }
    [ordered]@{
        warning='Private local safety copy, not a release/source distribution. Excludes ignored build/cache files and Git history.'
        head=(& git rev-parse HEAD); branch=(& git branch --show-current)
        status=@(& git --no-optional-locks -c core.quotepath=false status --short)
        files=@($manifest)
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $Destination 'BACKUP_MANIFEST.json') -Encoding utf8
    Write-Output ('Verified local backup: ' + $Destination)
    Write-Output ('Files: ' + @($manifest).Count)
} finally { Pop-Location }
