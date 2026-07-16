$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

function Fail($message) {
    throw $message
}

function Assert-File($path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Fail "Required file is missing: $path"
    }
}

function Get-TextFiles {
    git ls-files | Where-Object {
        $_ -match '\.(md|yml|yaml|toml|rs|js|css|html|json|ps1)$' -or
        $_ -in @('LICENSE', 'justfile', 'Dockerfile.sync-server')
    }
}

$requiredFiles = @(
    'LICENSE',
    'CONTRIBUTING.md',
    'SECURITY.md',
    'CODE_OF_CONDUCT.md',
    'SUPPORT.md',
    'GOVERNANCE.md',
    'RELEASE.md',
    '.github/CODEOWNERS',
    '.github/PULL_REQUEST_TEMPLATE.md',
    '.github/ISSUE_TEMPLATE/bug.yml',
    '.github/ISSUE_TEMPLATE/feature.yml',
    '.github/ISSUE_TEMPLATE/config.yml',
    'docs/adr/README.md',
    'docs/adr/0000-template.md'
)

foreach ($file in $requiredFiles) {
    Assert-File $file
}

$rootCargo = Get-Content -Raw Cargo.toml
if ($rootCargo -notmatch '(?m)^edition\s*=\s*"2021"\s*$') {
    Fail 'Workspace edition must remain 2021.'
}
if ($rootCargo -notmatch '(?m)^version\s*=\s*"0\.2\.0"\s*$') {
    Fail 'Workspace version must remain 0.2.0 for this baseline.'
}
if ($rootCargo -notmatch '(?m)^license\s*=\s*"MIT"\s*$') {
    Fail 'Workspace license must remain MIT.'
}

$cargoFiles = git ls-files '*Cargo.toml'
foreach ($file in $cargoFiles) {
    $text = Get-Content -Raw $file
    if ($file -eq 'Cargo.toml') {
        continue
    }
    if ($text -notmatch '(?m)^license\.workspace\s*=\s*true\s*$' -and $text -notmatch '(?m)^license\s*=\s*"MIT"\s*$') {
        Fail "Cargo license metadata is not MIT/workspace inherited in $file"
    }
}

$issueForms = @(
    '.github/ISSUE_TEMPLATE/bug.yml',
    '.github/ISSUE_TEMPLATE/feature.yml',
    '.github/ISSUE_TEMPLATE/config.yml'
)

foreach ($file in $issueForms) {
    $text = Get-Content -Raw $file
    if ($text -match "`t") {
        Fail "YAML file contains tabs: $file"
    }
    if ($file -ne '.github/ISSUE_TEMPLATE/config.yml') {
        foreach ($key in @('name:', 'description:', 'title:', 'labels:', 'body:')) {
            if ($text -notmatch "(?m)^$([regex]::Escape($key))") {
                Fail "Issue form $file is missing $key"
            }
        }
    }
}

$ruby = Get-Command ruby -ErrorAction SilentlyContinue
if ($ruby) {
    & ruby -e "require 'yaml'; ARGV.each { |f| YAML.load_file(f) }" @issueForms
    if ($LASTEXITCODE -ne 0) {
        Fail 'Issue-template YAML parsing failed.'
    }
} else {
    Write-Host 'Ruby is not installed; performed structural YAML checks only.'
}

$prTemplate = Get-Content -Raw '.github/PULL_REQUEST_TEMPLATE.md'
foreach ($heading in @(
    'Linked Issue',
    'Summary',
    'Scope',
    'Architecture Impact',
    'API Impact',
    'Data Or Migration Impact',
    'Security And Privacy Impact',
    'Testing Performed',
    'Documentation Changes',
    'Rollback Or Recovery Considerations',
    'Screenshots',
    'Credentials And Production Data'
)) {
    if ($prTemplate -notmatch "(?m)^## $([regex]::Escape($heading))\s*$") {
        Fail "Pull request template is missing heading: $heading"
    }
}

$secretPatterns = @(
    '(^|/)\.env($|[.])',
    '(^|/)id_rsa($|[.])',
    '(^|/)id_ed25519($|[.])',
    '\.(pem|p12|pfx|key)$',
    '(^|/)(credentials|secrets)\.(json|toml|yml|yaml|env)$'
)

$trackedFiles = git ls-files
foreach ($file in $trackedFiles) {
    $normalized = $file -replace '\\', '/'
    if ($normalized -eq '.env.example') {
        continue
    }
    foreach ($pattern in $secretPatterns) {
        if ($normalized -match $pattern) {
            Fail "Obvious secret-like file is tracked: $file"
        }
    }
}

$markdownFiles = git ls-files '*.md'
$linkPattern = '\[[^\]]+\]\(([^)\s]+)(?:\s+"[^"]*")?\)'
foreach ($file in $markdownFiles) {
    $text = Get-Content -Raw $file
    $matches = [regex]::Matches($text, $linkPattern)
    $baseDir = Split-Path -Parent $file
    foreach ($match in $matches) {
        $target = $match.Groups[1].Value
        if ($target -match '^(https?|mailto):' -or $target -match '^#') {
            continue
        }
        $pathOnly = ($target -split '#')[0]
        if ([string]::IsNullOrWhiteSpace($pathOnly)) {
            continue
        }
        $decoded = [Uri]::UnescapeDataString($pathOnly)
        $candidate = if ([string]::IsNullOrEmpty($baseDir)) { $decoded } else { Join-Path $baseDir $decoded }
        if (-not (Test-Path -LiteralPath $candidate)) {
            Fail "Broken Markdown link in ${file}: $target"
        }
    }
}

foreach ($file in Get-TextFiles) {
    $lines = Get-Content -LiteralPath $file
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '\s+$') {
            Fail "Trailing whitespace in $file at line $($i + 1)"
        }
    }
}

Write-Host 'Governance validation passed.'
