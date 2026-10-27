param([string]$Destination = (Join-Path ([System.IO.Path]::GetTempPath()) ('branchroom-demo-' + [guid]::NewGuid().ToString('N'))))
$ErrorActionPreference = 'Stop'
if (Test-Path -LiteralPath $Destination) { throw 'Choose a directory that does not exist.' }
New-Item -ItemType Directory -Path $Destination | Out-Null
function Invoke-DemoGit {
    & git -C $Destination @args | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Git failed: $args" }
}
function Write-DemoFile([string]$Name, [string]$Text) {
    [System.IO.File]::WriteAllText((Join-Path $Destination $Name), $Text)
}
Invoke-DemoGit init -b main
Invoke-DemoGit config user.name 'Demo Developer'
Invoke-DemoGit config user.email 'demo@example.invalid'
Invoke-DemoGit config commit.gpgsign false
Write-DemoFile README.md "# Fieldnotes`nA small sample project for exploring Branchroom.`n"
Write-DemoFile app.rs "fn main() { println!(`"Fieldnotes`" ); }`n"
Write-DemoFile .gitignore "target/`n.env`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Create the Fieldnotes workspace'
Invoke-DemoGit tag v0.1.0
Invoke-DemoGit switch -c feature/search
Write-DemoFile search.rs "pub fn search(query: &str) -> String { query.trim().to_owned() }`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Add search query normalization'
Write-DemoFile search.rs "pub fn search(query: &str) -> String { query.trim().to_lowercase() }`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Match notes without case sensitivity'
Invoke-DemoGit switch main
Write-DemoFile settings.toml "theme = 'paper'`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Choose the default workspace theme'
Invoke-DemoGit merge --no-ff feature/search -m 'Merge note search'
Invoke-DemoGit switch -c feature/offline
Write-DemoFile cache.rs "pub const CACHE_LIMIT: usize = 128;`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Keep recent notes available offline'
Invoke-DemoGit switch main
Write-DemoFile CHANGELOG.md "## 0.2`n- Search notes by text`n"
Invoke-DemoGit add -A
Invoke-DemoGit commit -m 'Document search in the release notes'
Invoke-DemoGit tag v0.2.0
Invoke-DemoGit switch feature/offline
Write-DemoFile scratch.txt "An unfinished export experiment`n"
Invoke-DemoGit stash push -u -m 'Try a plain text export'
Write-DemoFile cache.rs "pub const CACHE_LIMIT: usize = 256;`n"
Write-DemoFile design-notes.md "# Experiment`nCompare a larger offline cache.`n"
Write-Output $Destination
