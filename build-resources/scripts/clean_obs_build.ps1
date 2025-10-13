# Removes OBS dependencies that are irrelevant to our use case.
# This is a destructive operation, so ensure your target directory is empty first!

# Remove unnecessary files and folders
Write-Host "Cleaning up unnecessary files..."

# Remove all .pdb files recursively
Get-ChildItem -Path "dist\" -Filter "*.pdb" -Recurse | Remove-Item -Force
Write-Host "Removed all .pdb files"

# Remove specific files and folders
$itemsToRemove = @(
    "dist\obs-plugins\64bit\rtmp-services.dll",
    "dist\data\obs-plugins\obs-transitions",
    "dist\obs-plugins\64bit\obs-transitions.dll"
)

foreach ($item in $itemsToRemove) {
    if (Test-Path $item) {
        if ((Get-Item $item) -is [System.IO.DirectoryInfo]) {
            Remove-Item -Path $item -Recurse -Force
            Write-Host "Removed directory: $item"
        } else {
            Remove-Item -Path $item -Force
            Write-Host "Removed file: $item"
        }
    } else {
        Write-Host "Item not found: $item"
    }
}

Write-Host "Cleanup completed!"
