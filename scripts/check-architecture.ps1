[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-HectorExpectedPackages {
    $packages = [ordered]@{}
    $packages['hector'] = [pscustomobject]@{
        ManifestPath = 'apps/hector/Cargo.toml'
        TargetKind   = 'bin'
    }
    $packages['hector-core'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-core/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-protocol'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-protocol/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-audio'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-audio/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-runtime'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-runtime/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-platform-windows'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-platform-windows/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-storage'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-storage/Cargo.toml'
        TargetKind   = 'lib'
    }
    $packages['hector-tui'] = [pscustomobject]@{
        ManifestPath = 'crates/hector-tui/Cargo.toml'
        TargetKind   = 'lib'
    }

    return $packages
}

function ConvertTo-HectorNormalizedPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    return $Path.Replace('\', '/')
}

function Test-HectorManifestLintContent {
    <#
    This is intentionally not a general TOML parser. It validates only the
    H04-owned workspace lint section/key and package lint-inheritance
    section/key. All unrelated TOML content is ignored.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Content,

        [Parameter(Mandatory)]
        [ValidateSet('Root', 'Package')]
        [string] $ManifestKind,

        [Parameter(Mandatory)]
        [string] $Label
    )

    $violations = [System.Collections.Generic.List[string]]::new()
    if ($ManifestKind -eq 'Root') {
        $targetSection = 'workspace.lints.rust'
        $targetKey = 'unsafe_op_in_unsafe_fn'
        $expectedValue = '"deny"'
    }
    else {
        $targetSection = 'lints'
        $targetKey = 'workspace'
        $expectedValue = 'true'
    }

    $sectionPattern = '^\[(?<section>[A-Za-z0-9_.-]+)\]$'
    $assignmentPattern = '^(?<key>[A-Za-z0-9_.-]+)\s*=\s*(?<value>.+?)\s*$'
    $currentSection = ''
    $targetSectionCount = 0
    $targetKeyCount = 0
    $lineNumber = 0

    foreach ($rawLine in [regex]::Split($Content, '\r?\n')) {
        $lineNumber++
        $line = $rawLine.Trim()
        if (($line.Length -eq 0) -or $line.StartsWith('#')) {
            continue
        }

        if ($line -match $sectionPattern) {
            $currentSection = $Matches.section
            if ($currentSection -eq $targetSection) {
                $targetSectionCount++
            }
            continue
        }

        if ($line.StartsWith('[')) {
            if ($line -match [regex]::Escape($targetSection)) {
                [void]$violations.Add(
                    "$Label line $lineNumber has a malformed [$targetSection] declaration."
                )
            }
            $currentSection = ''
            continue
        }

        if ($line -match $assignmentPattern) {
            $key = $Matches.key
            $value = $Matches.value
            if ($key -eq $targetKey) {
                if ($currentSection -ne $targetSection) {
                    [void]$violations.Add(
                        "$Label line $lineNumber places '$targetKey' outside [$targetSection]."
                    )
                }
                else {
                    $targetKeyCount++
                    if ($value -cne $expectedValue) {
                        [void]$violations.Add(
                            "$Label line $lineNumber requires '$targetKey = $expectedValue'."
                        )
                    }
                }
            }
            continue
        }

        if ($currentSection -eq $targetSection) {
            [void]$violations.Add(
                "$Label line $lineNumber is malformed inside [$targetSection]."
            )
        }
    }

    if ($targetSectionCount -eq 0) {
        [void]$violations.Add("$Label is missing [$targetSection].")
    }
    elseif ($targetSectionCount -gt 1) {
        [void]$violations.Add("$Label contains duplicate [$targetSection] sections.")
    }

    if ($targetKeyCount -eq 0) {
        [void]$violations.Add(
            "$Label is missing '$targetKey = $expectedValue' in [$targetSection]."
        )
    }
    elseif ($targetKeyCount -gt 1) {
        [void]$violations.Add(
            "$Label contains duplicate '$targetKey' keys in [$targetSection]."
        )
    }

    return $violations.ToArray()
}

function Test-HectorManifestLintFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [ValidateSet('Root', 'Package')]
        [string] $ManifestKind,

        [Parameter(Mandatory)]
        [string] $Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return @("$Label does not exist at '$Path'.")
    }

    try {
        $utf8 = [System.Text.UTF8Encoding]::new($false, $true)
        $content = [System.IO.File]::ReadAllText($Path, $utf8)
    }
    catch {
        return @("$Label could not be read as UTF-8: $($_.Exception.Message)")
    }

    return @(
        Test-HectorManifestLintContent `
            -Content $content `
            -ManifestKind $ManifestKind `
            -Label $Label
    )
}

function Get-HectorCargoMetadata {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $RepositoryRoot
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = 'cargo'
    [void]$startInfo.ArgumentList.Add('metadata')
    [void]$startInfo.ArgumentList.Add('--locked')
    [void]$startInfo.ArgumentList.Add('--format-version')
    [void]$startInfo.ArgumentList.Add('1')
    $startInfo.WorkingDirectory = $RepositoryRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw 'Cargo metadata process did not start.'
    }

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()

    if ($process.ExitCode -ne 0) {
        throw "cargo metadata failed with exit $($process.ExitCode): $stderr"
    }

    try {
        return $stdout | ConvertFrom-Json -Depth 100
    }
    catch {
        throw "cargo metadata returned invalid JSON: $($_.Exception.Message)"
    }
}

function ConvertTo-HectorArchitectureModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [object] $Metadata
    )

    $workspaceMemberIds = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::Ordinal
    )
    foreach ($memberId in @($Metadata.workspace_members)) {
        [void]$workspaceMemberIds.Add([string]$memberId)
    }

    $workspacePackages = @(
        $Metadata.packages |
            Where-Object { $workspaceMemberIds.Contains([string]$_.id) }
    )
    $workspaceNames = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::Ordinal
    )
    foreach ($package in $workspacePackages) {
        [void]$workspaceNames.Add([string]$package.name)
    }

    $packages = foreach ($package in $workspacePackages) {
        $dependencies = foreach ($dependency in @($package.dependencies)) {
            $kind = if ($null -eq $dependency.kind) {
                'normal'
            }
            else {
                [string]$dependency.kind
            }

            [pscustomobject]@{
                Name        = [string]$dependency.name
                Kind        = $kind
                IsWorkspace = $workspaceNames.Contains([string]$dependency.name)
            }
        }

        $featureNames = @(
            $package.features.PSObject.Properties |
                ForEach-Object { [string]$_.Name }
        )
        $targetKinds = @(
            $package.targets |
                ForEach-Object { $_.kind } |
                ForEach-Object { [string]$_ }
        )
        $hasCustomBuild = @(
            $package.targets |
                Where-Object { $_.kind -contains 'custom-build' }
        ).Count -gt 0
        $relativeManifest = [System.IO.Path]::GetRelativePath(
            [string]$Metadata.workspace_root,
            [string]$package.manifest_path
        )
        $publishDisabled = (
            ($null -ne $package.PSObject.Properties['publish']) -and
            ($null -ne $package.publish) -and
            (@($package.publish).Count -eq 0)
        )

        [pscustomobject]@{
            Name                   = [string]$package.name
            ManifestPath           = ConvertTo-HectorNormalizedPath $relativeManifest
            Edition                = [string]$package.edition
            PublishDisabled        = $publishDisabled
            TargetKinds            = $targetKinds
            DependencyDeclarations = @($dependencies)
            FeatureNames           = $featureNames
            HasCustomBuild         = $hasCustomBuild
        }
    }

    $rootManifest = [System.IO.Path]::GetFullPath(
        [System.IO.Path]::Combine([string]$Metadata.workspace_root, 'Cargo.toml')
    )
    $rootIsPackage = @(
        $workspacePackages |
            Where-Object {
                [System.IO.Path]::GetFullPath([string]$_.manifest_path) -eq $rootManifest
            }
    ).Count -gt 0

    return [pscustomobject]@{
        IsVirtualRoot = -not $rootIsPackage
        Packages      = @($packages)
    }
}

function Get-HectorCycleViolations {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [object[]] $Packages
    )

    $nodes = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::Ordinal
    )
    foreach ($package in $Packages) {
        [void]$nodes.Add([string]$package.Name)
    }

    $adjacency = @{}
    $inDegree = @{}
    foreach ($name in $nodes) {
        $adjacency[$name] = [System.Collections.Generic.List[string]]::new()
        $inDegree[$name] = 0
    }

    foreach ($package in $Packages) {
        foreach ($dependency in @($package.DependencyDeclarations)) {
            if (
                $dependency.IsWorkspace -and
                $nodes.Contains([string]$dependency.Name)
            ) {
                $adjacency[[string]$package.Name].Add([string]$dependency.Name)
                $inDegree[[string]$dependency.Name]++
            }
        }
    }

    $queue = [System.Collections.Generic.Queue[string]]::new()
    foreach ($name in $nodes) {
        if ($inDegree[$name] -eq 0) {
            $queue.Enqueue($name)
        }
    }

    $visited = 0
    while ($queue.Count -gt 0) {
        $name = $queue.Dequeue()
        $visited++
        foreach ($dependencyName in $adjacency[$name]) {
            $inDegree[$dependencyName]--
            if ($inDegree[$dependencyName] -eq 0) {
                $queue.Enqueue($dependencyName)
            }
        }
    }

    if ($visited -ne $nodes.Count) {
        $remaining = @(
            $nodes |
                Where-Object { $inDegree[$_] -gt 0 } |
                Sort-Object
        )
        return @(
            "Workspace dependency cycle detected among: $($remaining -join ', ')."
        )
    }

    return @()
}

function Test-HectorArchitectureModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [object] $Model
    )

    $violations = [System.Collections.Generic.List[string]]::new()
    $expectedPackages = Get-HectorExpectedPackages
    $actualPackages = @($Model.Packages)

    if (-not $Model.IsVirtualRoot) {
        [void]$violations.Add('The workspace root must remain virtual.')
    }

    foreach ($duplicate in @($actualPackages | Group-Object Name | Where-Object Count -gt 1)) {
        [void]$violations.Add("Workspace package '$($duplicate.Name)' is duplicated.")
    }

    $actualNames = @($actualPackages.Name)
    foreach ($expectedName in $expectedPackages.Keys) {
        if ($expectedName -notin $actualNames) {
            [void]$violations.Add("Required workspace package '$expectedName' is missing.")
        }
    }
    foreach ($actualName in $actualNames) {
        if (-not $expectedPackages.Contains($actualName)) {
            [void]$violations.Add("Unexpected workspace package '$actualName' exists.")
        }
    }

    foreach ($package in $actualPackages) {
        if (-not $expectedPackages.Contains($package.Name)) {
            continue
        }

        $expected = $expectedPackages[$package.Name]
        $actualPath = ConvertTo-HectorNormalizedPath ([string]$package.ManifestPath)
        if ($actualPath -cne $expected.ManifestPath) {
            [void]$violations.Add(
                "Package '$($package.Name)' must use manifest '$($expected.ManifestPath)', found '$actualPath'."
            )
        }
        if ([string]$package.Edition -cne '2024') {
            [void]$violations.Add("Package '$($package.Name)' must use edition 2024.")
        }
        if (-not $package.PublishDisabled) {
            [void]$violations.Add("Package '$($package.Name)' must set publish = false.")
        }

        $targetKinds = @($package.TargetKinds | Sort-Object -Unique)
        if (
            ($targetKinds.Count -ne 1) -or
            ($targetKinds[0] -cne $expected.TargetKind)
        ) {
            [void]$violations.Add(
                "Package '$($package.Name)' must have only target kind '$($expected.TargetKind)'."
            )
        }

        foreach ($dependency in @($package.DependencyDeclarations)) {
            $scope = if ($dependency.IsWorkspace) { 'workspace' } else { 'external' }
            [void]$violations.Add(
                "H04 forbids $scope dependency '$($package.Name) -> $($dependency.Name)' ($($dependency.Kind))."
            )
        }
        foreach ($featureName in @($package.FeatureNames)) {
            [void]$violations.Add(
                "H04 forbids feature '$featureName' in package '$($package.Name)'."
            )
        }
        if ($package.HasCustomBuild) {
            [void]$violations.Add(
                "H04 forbids a custom build target in package '$($package.Name)'."
            )
        }
    }

    foreach ($cycleViolation in @(Get-HectorCycleViolations -Packages $actualPackages)) {
        [void]$violations.Add($cycleViolation)
    }

    return $violations.ToArray()
}

function Invoke-HectorArchitectureCheck {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $RepositoryRoot
    )

    $violations = [System.Collections.Generic.List[string]]::new()
    $expectedPackages = Get-HectorExpectedPackages

    try {
        $metadata = Get-HectorCargoMetadata -RepositoryRoot $RepositoryRoot
        $model = ConvertTo-HectorArchitectureModel -Metadata $metadata
        foreach ($violation in @(Test-HectorArchitectureModel -Model $model)) {
            [void]$violations.Add($violation)
        }
    }
    catch {
        [void]$violations.Add("Cargo metadata validation failed: $($_.Exception.Message)")
    }

    $rootManifest = Join-Path $RepositoryRoot 'Cargo.toml'
    foreach (
        $violation in @(
            Test-HectorManifestLintFile `
                -Path $rootManifest `
                -ManifestKind Root `
                -Label 'Root Cargo.toml'
        )
    ) {
        [void]$violations.Add($violation)
    }

    foreach ($packageName in $expectedPackages.Keys) {
        $manifestPath = Join-Path `
            $RepositoryRoot `
            ($expectedPackages[$packageName].ManifestPath.Replace('/', '\'))
        foreach (
            $violation in @(
                Test-HectorManifestLintFile `
                    -Path $manifestPath `
                    -ManifestKind Package `
                    -Label "$packageName Cargo.toml"
            )
        ) {
            [void]$violations.Add($violation)
        }
    }

    return $violations.ToArray()
}

if ($MyInvocation.InvocationName -ne '.') {
    $repositoryRoot = Split-Path -Parent $PSScriptRoot
    try {
        $architectureViolations = @(
            Invoke-HectorArchitectureCheck -RepositoryRoot $repositoryRoot
        )
    }
    catch {
        $architectureViolations = @(
            "Architecture checker failed unexpectedly: $($_.Exception.Message)"
        )
    }

    if ($architectureViolations.Count -eq 0) {
        Write-Output 'Hector architecture check: PASS'
        exit 0
    }

    Write-Output "Hector architecture check: FAIL ($($architectureViolations.Count) violations)"
    foreach ($violation in $architectureViolations) {
        Write-Output " - $violation"
    }
    exit 1
}
