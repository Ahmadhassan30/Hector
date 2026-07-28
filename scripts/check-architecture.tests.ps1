[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'check-architecture.ps1')

$script:Passed = 0
$script:Failed = 0

function Invoke-HectorTestCase {
    param(
        [Parameter(Mandatory)]
        [string] $Name,

        [Parameter(Mandatory)]
        [scriptblock] $Test
    )

    try {
        & $Test
        $script:Passed++
        Write-Output "PASS: $Name"
    }
    catch {
        $script:Failed++
        Write-Output "FAIL: $Name"
        Write-Output "  $($_.Exception.Message)"
    }
}

function Assert-HectorNoViolations {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [object[]] $Violations
    )

    if ($Violations.Count -ne 0) {
        throw "Expected no violations, found: $($Violations -join ' | ')"
    }
}

function Assert-HectorViolation {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [object[]] $Violations,

        [Parameter(Mandatory)]
        [string] $Pattern
    )

    if (($Violations -join "`n") -notmatch $Pattern) {
        throw "Expected a violation matching '$Pattern', found: $($Violations -join ' | ')"
    }
}

function New-HectorTestModel {
    $packages = foreach ($entry in (Get-HectorExpectedPackages).GetEnumerator()) {
        [pscustomobject]@{
            Name                   = [string]$entry.Key
            ManifestPath           = [string]$entry.Value.ManifestPath
            Edition                = '2024'
            PublishDisabled        = $true
            TargetKinds            = @([string]$entry.Value.TargetKind)
            DependencyDeclarations = @()
            FeatureNames           = @()
            HasCustomBuild         = $false
        }
    }

    return [pscustomobject]@{
        IsVirtualRoot = $true
        Packages      = @($packages)
    }
}

function Get-HectorTestPackage {
    param(
        [Parameter(Mandatory)]
        [object] $Model,

        [Parameter(Mandatory)]
        [string] $Name
    )

    return @($Model.Packages | Where-Object Name -CEQ $Name)[0]
}

function Add-HectorTestDependency {
    param(
        [Parameter(Mandatory)]
        [object] $Model,

        [Parameter(Mandatory)]
        [string] $Consumer,

        [Parameter(Mandatory)]
        [string] $Dependency,

        [Parameter(Mandatory)]
        [bool] $IsWorkspace
    )

    $package = Get-HectorTestPackage -Model $Model -Name $Consumer
    $package.DependencyDeclarations = @(
        $package.DependencyDeclarations
    ) + @(
        [pscustomobject]@{
            Name        = $Dependency
            Kind        = 'normal'
            IsWorkspace = $IsWorkspace
        }
    )
}

Invoke-HectorTestCase 'edgeless H04 workspace passes' {
    $violations = @(Test-HectorArchitectureModel -Model (New-HectorTestModel))
    Assert-HectorNoViolations -Violations $violations
}

$forbiddenInternalEdges = @(
    @('hector-runtime', 'hector-platform-windows'),
    @('hector-runtime', 'hector-storage'),
    @('hector-runtime', 'hector-audio'),
    @('hector', 'hector-runtime'),
    @('hector', 'hector-tui'),
    @('hector-protocol', 'hector-core'),
    @('hector-audio', 'hector-core')
)
foreach ($edge in $forbiddenInternalEdges) {
    $consumer = $edge[0]
    $dependency = $edge[1]
    Invoke-HectorTestCase "H04 rejects $consumer -> $dependency" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer $consumer `
            -Dependency $dependency `
            -IsWorkspace $true
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'H04 forbids workspace dependency'
    }
}

Invoke-HectorTestCase 'external dependency is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-runtime' `
        -Dependency 'tokio' `
        -IsWorkspace $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'H04 forbids external dependency'
}

Invoke-HectorTestCase 'unknown package is rejected' {
    $model = New-HectorTestModel
    $model.Packages = @($model.Packages) + @(
        [pscustomobject]@{
            Name                   = 'hector-premature'
            ManifestPath           = 'crates/hector-premature/Cargo.toml'
            Edition                = '2024'
            PublishDisabled        = $true
            TargetKinds            = @('lib')
            DependencyDeclarations = @()
            FeatureNames           = @()
            HasCustomBuild         = $false
        }
    )
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Unexpected workspace package'
}

Invoke-HectorTestCase 'missing package is rejected' {
    $model = New-HectorTestModel
    $model.Packages = @(
        $model.Packages | Where-Object Name -CNE 'hector-core'
    )
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Required workspace package'
}

Invoke-HectorTestCase 'feature is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').FeatureNames = @('default')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'H04 forbids feature'
}

Invoke-HectorTestCase 'custom build target is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').HasCustomBuild = $true
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'H04 forbids a custom build target'
}

Invoke-HectorTestCase 'incorrect manifest path is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').ManifestPath = 'wrong/Cargo.toml'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use manifest'
}

Invoke-HectorTestCase 'incorrect target kind is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').TargetKinds = @('bin')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must have only target kind'
}

Invoke-HectorTestCase 'incorrect edition is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').Edition = '2021'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use edition 2024'
}

Invoke-HectorTestCase 'publishing enabled is rejected' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-core').PublishDisabled = $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must set publish = false'
}

Invoke-HectorTestCase 'non-virtual root is rejected' {
    $model = New-HectorTestModel
    $model.IsVirtualRoot = $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'workspace root must remain virtual'
}

Invoke-HectorTestCase 'workspace dependency cycle is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-core' `
        -Dependency 'hector-runtime' `
        -IsWorkspace $true
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-runtime' `
        -Dependency 'hector-core' `
        -IsWorkspace $true
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'cycle detected'
}

Invoke-HectorTestCase 'multiple violations are aggregated' {
    $model = New-HectorTestModel
    $package = Get-HectorTestPackage -Model $model -Name 'hector-core'
    $package.Edition = '2021'
    $package.FeatureNames = @('default')
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-core' `
        -Dependency 'serde' `
        -IsWorkspace $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    if ($violations.Count -lt 3) {
        throw "Expected at least three violations, found $($violations.Count)."
    }
}

$validRootManifest = @'
[workspace]
members = []

[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "deny"
'@
$validPackageManifest = @'
[package]
name = "hector-core"

[lints]
workspace = true
'@

Invoke-HectorTestCase 'valid root lint declaration passes' {
    $violations = @(
        Test-HectorManifestLintContent `
            -Content $validRootManifest `
            -ManifestKind Root `
            -Label 'root'
    )
    Assert-HectorNoViolations -Violations $violations
}

Invoke-HectorTestCase 'valid package lint inheritance passes' {
    $violations = @(
        Test-HectorManifestLintContent `
            -Content $validPackageManifest `
            -ManifestKind Package `
            -Label 'package'
    )
    Assert-HectorNoViolations -Violations $violations
}

$manifestFailureCases = @(
    @{
        Name = 'missing root lint section fails'
        Kind = 'Root'
        Content = '[workspace]'
        Pattern = 'missing \[workspace\.lints\.rust\]'
    },
    @{
        Name = 'missing package lint key fails'
        Kind = 'Package'
        Content = '[lints]'
        Pattern = "missing 'workspace = true'"
    },
    @{
        Name = 'false package lint inheritance fails'
        Kind = 'Package'
        Content = "[lints]`nworkspace = false"
        Pattern = 'requires'
    },
    @{
        Name = 'misplaced package lint key fails'
        Kind = 'Package'
        Content = "[package]`nworkspace = true`n`n[lints]"
        Pattern = 'outside \[lints\]'
    },
    @{
        Name = 'duplicate package lint section fails'
        Kind = 'Package'
        Content = "[lints]`nworkspace = true`n`n[lints]`nworkspace = true"
        Pattern = 'duplicate \[lints\]'
    },
    @{
        Name = 'duplicate package lint key fails'
        Kind = 'Package'
        Content = "[lints]`nworkspace = true`nworkspace = true"
        Pattern = "duplicate 'workspace'"
    },
    @{
        Name = 'malformed package lint section fails'
        Kind = 'Package'
        Content = "[lints`nworkspace = true"
        Pattern = 'malformed \[lints\]'
    },
    @{
        Name = 'malformed package lint assignment fails'
        Kind = 'Package'
        Content = "[lints]`nworkspace true"
        Pattern = 'malformed inside \[lints\]'
    }
)

foreach ($case in $manifestFailureCases) {
    Invoke-HectorTestCase $case.Name {
        $violations = @(
            Test-HectorManifestLintContent `
                -Content $case.Content `
                -ManifestKind $case.Kind `
                -Label 'manifest'
        )
        Assert-HectorViolation -Violations $violations -Pattern $case.Pattern
    }
}

Write-Output "Hector architecture checker tests: $script:Passed passed, $script:Failed failed"
if ($script:Failed -gt 0) {
    exit 1
}
exit 0
