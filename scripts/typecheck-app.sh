#!/usr/bin/env bash
# Type-checks the WinUI app's C# on a machine that cannot build WinUI.
#
# The XAML compiler only runs on Windows, so `dotnet build` on the app project stops
# before the C# compiler ever sees the code — which means a Linux or macOS contributor
# has no way to catch a typo without pushing and waiting for the Windows CI job.
#
# This works around that by generating the partial-class members the XAML compiler would
# have emitted (the x:Name fields and InitializeComponent), then compiling every .cs file
# of the app against them. It checks the C# only: real XAML errors, binding paths and
# missing resources are still the Windows job's business.
#
#   ./scripts/typecheck-app.sh
#
# Exits non-zero if the app's C# does not compile.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_dir="$repo_root/src/SafeInvest.App"
work_dir="${TMPDIR:-/tmp}/safeinvest-typecheck"

rm -rf "$work_dir"
mkdir -p "$work_dir"

python3 "$repo_root/scripts/gen-xaml-stubs.py" "$app_dir" "$work_dir/stubs"

cat > "$work_dir/typecheck.csproj" <<PROJECT
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0-windows10.0.26100.0</TargetFramework>
    <TargetPlatformMinVersion>10.0.17763.0</TargetPlatformMinVersion>
    <RootNamespace>SafeInvest.App</RootNamespace>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <LangVersion>latest</LangVersion>
    <UseWinUI>true</UseWinUI>
    <WindowsPackageType>None</WindowsPackageType>
    <EnableWindowsTargeting>true</EnableWindowsTargeting>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <EnableDefaultItems>false</EnableDefaultItems>
    <ManagePackageVersionsCentrally>false</ManagePackageVersionsCentrally>
    <NoWarn>CA1416;CA1852;CA1305;CA1304;CA1310;CA1720;CS0067</NoWarn>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.WindowsAppSDK" Version="2.4.0" />
    <PackageReference Include="Microsoft.Windows.SDK.BuildTools" Version="10.0.26100.9169" />
    <PackageReference Include="CommunityToolkit.Mvvm" Version="8.4.2" />
    <PackageReference Include="Microsoft.Extensions.Hosting" Version="10.0.11" />
  </ItemGroup>
  <ItemGroup>
    <ProjectReference Include="$repo_root/src/SafeInvest.Core/SafeInvest.Core.csproj" />
    <ProjectReference Include="$repo_root/src/SafeInvest.MarketData/SafeInvest.MarketData.csproj" />
  </ItemGroup>
  <ItemGroup>
    <Compile Include="stubs/*.cs" />
    <Compile Include="$app_dir/**/*.cs"
             Exclude="$app_dir/obj/**/*.cs;$app_dir/bin/**/*.cs" />
  </ItemGroup>
</Project>
PROJECT

echo "Compilation du C# de l'application…"

# The build always fails at the end on Windows-only resource generation (MakePri), long
# after the C# compiler has run. Only the compiler diagnostics matter here.
log="$work_dir/build.log"
set +e
dotnet build "$work_dir/typecheck.csproj" -v q > "$log" 2>&1
set -e

if grep -qE "error CS" "$log"; then
    echo
    echo "Erreurs de compilation dans src/SafeInvest.App :"
    grep -E "error CS" "$log" | sed "s|$app_dir/||; s| \[.*||" | sort -u
    exit 1
fi

echo "Le C# de l'application compile. (Le XAML lui-même reste vérifié par la CI Windows.)"
