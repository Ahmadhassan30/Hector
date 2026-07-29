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
            TargetKinds            = @($entry.Value.TargetKinds | ForEach-Object { [string]$_ })
            DependencyDeclarations = @()
            FeatureNames           = @()
            HasCustomBuild         = $false
        }
    }

    $model = [pscustomobject]@{
        IsVirtualRoot = $true
        Packages      = @($packages)
    }

    foreach ($consumer in @(
        'hector-protocol',
        'hector-audio',
        'hector-fake-worker',
        'hector-platform-windows'
    )) {
        foreach ($dependency in (Get-HectorExpectedDependencies)[$consumer]) {
            Add-HectorTestDependency `
                -Model $model `
                -Consumer $consumer `
                -Dependency $dependency.Name `
                -IsWorkspace $dependency.IsWorkspace `
                -Kind $dependency.Kind `
                -Features $dependency.Features `
                -Target $dependency.Target `
                -Requirement $dependency.Requirement
        }
    }

    return $model
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
        [bool] $IsWorkspace,

        [string] $Kind = 'normal',

        [string[]] $Features = @(),

        [string] $Target = '',

        [string] $Requirement = ''
    )

    $package = Get-HectorTestPackage -Model $Model -Name $Consumer
    $package.DependencyDeclarations = @(
        $package.DependencyDeclarations
    ) + @(
        [pscustomobject]@{
            Name                = $Dependency
            Kind                = $Kind
            IsWorkspace         = $IsWorkspace
            Features            = @($Features)
            UsesDefaultFeatures = $true
            Target              = $Target
            Requirement         = $Requirement
        }
    )
}

Invoke-HectorTestCase 'exact H13 through H16 dependency topology passes' {
    $violations = @(Test-HectorArchitectureModel -Model (New-HectorTestModel))
    Assert-HectorNoViolations -Violations $violations
}

$forbiddenInternalEdges = @(
    @('hector-runtime', 'hector-platform-windows'),
    @('hector-runtime', 'hector-storage'),
    @('hector-runtime', 'hector-audio'),
    @('hector', 'hector-runtime'),
    @('hector', 'hector-tui'),
    @('hector-audio', 'hector-protocol'),
    @('hector-core', 'hector-audio'),
    @('hector-audio', 'hector-fake-worker'),
    @('hector-core', 'hector-fake-worker'),
    @('hector-protocol', 'hector-fake-worker'),
    @('hector-fake-worker', 'hector-audio'),
    @('hector-fake-worker', 'hector-runtime'),
    @('hector-fake-worker', 'hector-storage'),
    @('hector-fake-worker', 'hector-tui'),
    @('hector-fake-worker', 'hector')
)
foreach ($edge in $forbiddenInternalEdges) {
    $consumer = $edge[0]
    $dependency = $edge[1]
    Invoke-HectorTestCase "architecture rejects $consumer -> $dependency" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer $consumer `
            -Dependency $dependency `
            -IsWorkspace $true
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids workspace dependency'
    }
}

Invoke-HectorTestCase 'fake worker production dependency on platform is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').DependencyDeclarations |
            Where-Object Name -CEQ 'hector-platform-windows'
    )[0]
    $dependency.Kind = 'normal'
    $dependency.Target = ''
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must be a dev workspace dependency'
}

Invoke-HectorTestCase 'fake worker platform dependency requires cfg windows' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').DependencyDeclarations |
            Where-Object Name -CEQ 'hector-platform-windows'
    )[0]
    $dependency.Target = ''
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern "target 'cfg\(windows\)'"
}

Invoke-HectorTestCase 'fake worker platform dependency features are rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').DependencyDeclarations |
            Where-Object Name -CEQ 'hector-platform-windows'
    )[0]
    $dependency.Features = @('unexpected')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
}

foreach ($requiredDependency in @('hector-protocol', 'windows-sys')) {
    Invoke-HectorTestCase "missing required platform dependency $requiredDependency is rejected" {
        $model = New-HectorTestModel
        $platform = Get-HectorTestPackage -Model $model -Name 'hector-platform-windows'
        $platform.DependencyDeclarations = @(
            $platform.DependencyDeclarations |
                Where-Object Name -CNE $requiredDependency
        )
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Required dependency'
    }
}

Invoke-HectorTestCase 'windows sys version requirement is exact' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-platform-windows').DependencyDeclarations |
            Where-Object Name -CEQ 'windows-sys'
    )[0]
    $dependency.Requirement = '^0.62.0'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern "version requirement '\^0\.61\.2'"
}

Invoke-HectorTestCase 'windows sys target condition is exact' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-platform-windows').DependencyDeclarations |
            Where-Object Name -CEQ 'windows-sys'
    )[0]
    $dependency.Target = ''
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern "target 'cfg\(windows\)'"
}

Invoke-HectorTestCase 'windows sys feature set is exact' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-platform-windows').DependencyDeclarations |
            Where-Object Name -CEQ 'windows-sys'
    )[0]
    $dependency.Features = @($dependency.Features) + @('Win32_UI_WindowsAndMessaging')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
}

foreach ($dependencyName in @(
    'tokio',
    'tracing',
    'serde',
    'cpal',
    'windows',
    'jobserver',
    'process-wrap'
)) {
    Invoke-HectorTestCase "platform external dependency $dependencyName is rejected" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer 'hector-platform-windows' `
            -Dependency $dependencyName `
            -IsWorkspace $false
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
    }
}

foreach ($dependencyName in @(
    'hector-core',
    'hector-audio',
    'hector-runtime',
    'hector-storage',
    'hector-tui',
    'hector',
    'hector-fake-worker'
)) {
    Invoke-HectorTestCase "platform dependency on $dependencyName is rejected" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer 'hector-platform-windows' `
            -Dependency $dependencyName `
            -IsWorkspace $true
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids workspace dependency'
    }
}

foreach ($dependencyName in @(
    'tokio',
    'serde',
    'serde_json',
    'base64',
    'tracing',
    'windows',
    'cpal',
    'reqwest'
)) {
    Invoke-HectorTestCase "fake worker external dependency $dependencyName is rejected" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer 'hector-fake-worker' `
            -Dependency $dependencyName `
            -IsWorkspace $false
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
    }
}

foreach ($requiredDependency in @(
    'hector-core',
    'hector-protocol',
    'hector-platform-windows'
)) {
    Invoke-HectorTestCase "missing required fake worker dependency $requiredDependency is rejected" {
        $model = New-HectorTestModel
        $worker = Get-HectorTestPackage -Model $model -Name 'hector-fake-worker'
        $worker.DependencyDeclarations = @(
            $worker.DependencyDeclarations |
                Where-Object Name -CNE $requiredDependency
        )
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Required dependency'
    }
}

Invoke-HectorTestCase 'fake worker dependency wrong kind is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').DependencyDeclarations |
            Where-Object Name -CEQ 'hector-protocol'
    )[0]
    $dependency.Kind = 'dev'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must be a normal workspace dependency'
}

Invoke-HectorTestCase 'fake worker dependency features are rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').DependencyDeclarations |
            Where-Object Name -CEQ 'hector-core'
    )[0]
    $dependency.Features = @('unexpected')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
}

Invoke-HectorTestCase 'fake worker must retain exact library and binary targets' {
    $model = New-HectorTestModel
    (Get-HectorTestPackage -Model $model -Name 'hector-fake-worker').TargetKinds = @('bin', 'lib')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must have exactly target kinds'
}

Invoke-HectorTestCase 'external dependency is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-runtime' `
        -Dependency 'tokio' `
        -IsWorkspace $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
}

foreach ($dependencyName in @(
    'tokio',
    'cpal',
    'serde',
    'base64',
    'tracing',
    'windows',
    'symphonia',
    'rubato'
)) {
    Invoke-HectorTestCase "audio external dependency $dependencyName is rejected" {
        $model = New-HectorTestModel
        Add-HectorTestDependency `
            -Model $model `
            -Consumer 'hector-audio' `
            -Dependency $dependencyName `
            -IsWorkspace $false
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
    }
}

Invoke-HectorTestCase 'missing required audio core dependency is rejected' {
    $model = New-HectorTestModel
    $audio = Get-HectorTestPackage -Model $model -Name 'hector-audio'
    $audio.DependencyDeclarations = @()
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Required dependency'
}

Invoke-HectorTestCase 'audio core dependency wrong kind is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-audio').DependencyDeclarations
    )[0]
    $dependency.Kind = 'dev'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must be a normal workspace dependency'
}

Invoke-HectorTestCase 'audio core dependency features are rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-audio').DependencyDeclarations
    )[0]
    $dependency.Features = @('unexpected')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
}

Invoke-HectorTestCase 'reverse core -> protocol dependency is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-core' `
        -Dependency 'hector-protocol' `
        -IsWorkspace $true
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids workspace dependency'
}

Invoke-HectorTestCase 'serialization dependency in core is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-core' `
        -Dependency 'serde' `
        -IsWorkspace $false `
        -Features @('derive')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
}

Invoke-HectorTestCase 'unrelated internal protocol dependency is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-protocol' `
        -Dependency 'hector-audio' `
        -IsWorkspace $true
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids workspace dependency'
}

Invoke-HectorTestCase 'unrelated external protocol dependency is rejected' {
    $model = New-HectorTestModel
    Add-HectorTestDependency `
        -Model $model `
        -Consumer 'hector-protocol' `
        -Dependency 'bincode' `
        -IsWorkspace $false
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'Architecture forbids external dependency'
}

foreach ($requiredDependency in @('base64', 'hector-core', 'serde', 'serde_json')) {
    Invoke-HectorTestCase "missing required protocol dependency $requiredDependency is rejected" {
        $model = New-HectorTestModel
        $protocol = Get-HectorTestPackage -Model $model -Name 'hector-protocol'
        $protocol.DependencyDeclarations = @(
            $protocol.DependencyDeclarations |
                Where-Object Name -CNE $requiredDependency
        )
        $violations = @(Test-HectorArchitectureModel -Model $model)
        Assert-HectorViolation -Violations $violations -Pattern 'Required dependency'
    }
}

Invoke-HectorTestCase 'wrong dependency kind is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-protocol').DependencyDeclarations |
            Where-Object Name -CEQ 'serde_json'
    )[0]
    $dependency.Kind = 'dev'
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must be a normal external dependency'
}

Invoke-HectorTestCase 'missing serde derive feature is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-protocol').DependencyDeclarations |
            Where-Object Name -CEQ 'serde'
    )[0]
    $dependency.Features = @()
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
}

Invoke-HectorTestCase 'unauthorized dependency feature is rejected' {
    $model = New-HectorTestModel
    $dependency = @(
        (Get-HectorTestPackage -Model $model -Name 'hector-protocol').DependencyDeclarations |
            Where-Object Name -CEQ 'serde_json'
    )[0]
    $dependency.Features = @('preserve_order')
    $violations = @(Test-HectorArchitectureModel -Model $model)
    Assert-HectorViolation -Violations $violations -Pattern 'must use exactly features'
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
    Assert-HectorViolation -Violations $violations -Pattern 'must have exactly target kinds'
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
