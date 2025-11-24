# Removes OBS dependencies that are irrelevant to our use case.
# This is a destructive operation, so ensure your target directory can be reconstructed!

param(
    [string]$TargetFolder = "dist"
)

# Ensure the target folder path ends with a backslash for consistency
if (-not $TargetFolder.EndsWith("\")) {
    $TargetFolder += "\"
}

# Remove unnecessary files and folders
Write-Host "Cleaning up unnecessary files in: $TargetFolder"

# Remove all .pdb files recursively
Get-ChildItem -Path $TargetFolder -Filter "*.pdb" -Recurse | Remove-Item -Force
Write-Host "Removed all .pdb files"

# Remove specific files and folders
$itemsToRemove = @(
    # Crashes on some users' machines, not necessary for our use case
    "$TargetFolder" + "obs-plugins\64bit\win-dshow.dll"
    # Not necessary for our use case
    "$TargetFolder" + "obs-plugins\64bit\aja-output-ui.dll"
    "$TargetFolder" + "obs-plugins\64bit\aja.dll"
    "$TargetFolder" + "obs-plugins\64bit\rtmp-services.dll"
    "$TargetFolder" + "obs-plugins\64bit\vlc-video.dll"
    "$TargetFolder" + "obs-plugins\64bit\obs-text.dll"
    "$TargetFolder" + "obs-plugins\64bit\obs-transitions.dll"
    "$TargetFolder" + "data\obs-plugins\obs-transitions"
)

foreach ($item in $itemsToRemove) {
    if (Test-Path $item) {
        if ((Get-Item $item) -is [System.IO.DirectoryInfo]) {
            Remove-Item -Path $item -Recurse -Force
            Write-Host "Removed directory: $item"
        }
        else {
            Remove-Item -Path $item -Force
            Write-Host "Removed file: $item"
        }
    }
    else {
        Write-Host "Item not found: $item"
    }
}

Write-Host "Cleanup completed!"
