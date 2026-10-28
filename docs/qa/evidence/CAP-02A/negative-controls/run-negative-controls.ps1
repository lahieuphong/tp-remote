<#
  CAP-02A negative controls: each control applies ONE deliberate violation, runs the guard
  that must catch it, expects that guard to FAIL, then restores the file byte-for-byte and
  verifies the SHA-256. Cargo.lock is hash-checked before and after the whole run.
  Run from the repository root:
    powershell -NoProfile -File docs\qa\evidence\CAP-02A\negative-controls\run-negative-controls.ps1 [-Log <file>]
#>
param([string]$Log = "docs\qa\evidence\CAP-02A\negative-controls\negative-controls.log")
$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$log = $Log
"# CAP-02A negative controls, $(Get-Date -Format s)" | Out-File -Encoding utf8 $log
$lockBefore = (Get-FileHash Cargo.lock -Algorithm SHA256).Hash

function Append([string]$text) { return { param($c) $c.TrimEnd() + "`n`n" + $text + "`n" }.GetNewClosure() }
function Replace([string]$old, [string]$new) { return { param($c) if (-not $c.Contains($old)) { throw "mutation anchor not found" }; $c.Replace($old, $new) }.GetNewClosure() }

$controls = @(
  @{ id = "NC-CAP-1"; what = "disable the capture border"; file = "crates\tp-capture\src\wgc\stream.rs"
     mutate = (Append "#[allow(dead_code)]`nfn nc_border(session: &GraphicsCaptureSession) {`n    let _ = session.SetIsBorderRequired(false);`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline the_capture_border_is_never_disabled"; expect = "the_capture_border_is_never_disabled ... FAILED" },
  @{ id = "NC-CAP-2"; what = "second capture-session creation site"; file = "crates\tp-capture\src\wgc\stream.rs"
     mutate = (Append "#[allow(dead_code)]`nfn nc_session(pool: &Direct3D11CaptureFramePool, item: &GraphicsCaptureItem) {`n    let _ = pool.CreateCaptureSession(item);`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline capture_sessions_are_created_in_one_place"; expect = "capture_sessions_are_created_in_one_place ... FAILED" },
  @{ id = "NC-CAP-3"; what = "programmatic target outside testing.rs"; file = "crates\tp-capture\src\wgc\picker.rs"
     mutate = (Append "#[cfg(feature = `"test-support`")]`n#[allow(dead_code)]`nfn nc_bypass(item: GraphicsCaptureItem) -> WgcTarget {`n    WgcTarget::programmatic(item)`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline programmatic_capture_targets_are_test_only"; expect = "programmatic_capture_targets_are_test_only ... FAILED" },
  @{ id = "NC-CAP-4"; what = "second LocalPreviewGrant mint site"; file = "crates\tp-core\src\preview\host.rs"
     mutate = (Append "#[allow(dead_code)]`nfn nc_mint() -> LocalPreviewGrant {`n    LocalPreviewGrant::on_picker_selection(1, MonoTime::ZERO)`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline only_the_picker_completion_mints_a_preview_grant"; expect = "only_the_picker_completion_mints_a_preview_grant ... FAILED" },
  @{ id = "NC-CAP-5"; what = "unsafe code outside tp-capture::wgc"; file = "crates\tp-capture\src\scale.rs"
     mutate = (Append "#[allow(unsafe_code, dead_code, unused_unsafe)]`nfn nc_unsafe() {`n    unsafe {}`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline unsafe_code_is_confined_and_justified"; expect = "unsafe_code_is_confined_and_justified ... FAILED" },
  @{ id = "NC-CAP-6"; what = "file write in the preview path"; file = "crates\tp-core\src\preview\host.rs"
     mutate = (Append "#[allow(dead_code)]`nfn nc_write() {`n    let _ = std::fs::write(`"frame.bin`", b`"`");`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline the_capture_and_preview_path_writes_no_files"; expect = "the_capture_and_preview_path_writes_no_files ... FAILED" },
  @{ id = "NC-CAP-7"; what = "capture contract implemented outside tp-capture"; file = "crates\tp-core\src\preview\host.rs"
     mutate = (Append "#[allow(dead_code)]`nstruct NcFrame;`nimpl FrameReadback for NcFrame {`n    fn read_rgba(&self, max: PixelSize, _out: &mut Vec<u8>) -> Result<PixelSize, CoreError> {`n        Ok(max)`n    }`n}")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline production_platform_implementations_exist_only_where_allowed"; expect = "production_platform_implementations_exist_only_where_allowed ... FAILED" },
  @{ id = "NC-CAP-8"; what = "test-support enabled in the app build graph"; file = "crates\tp-core\Cargo.toml"
     mutate = (Replace "tp-capture = { workspace = true }" "tp-capture = { workspace = true, features = [`"test-support`"] }")
     test = "cargo test -p tp-test-harness --test workspace_boundaries --offline the_app_build_graph_has_no_test_support"; expect = "the_app_build_graph_has_no_test_support ... FAILED" },
  @{ id = "NC-CAP-9"; what = "LocalPreviewGrant made Clone"; file = "crates\tp-core\src\preview\mod.rs"
     mutate = (Replace "#[derive(Debug, PartialEq, Eq)]`npub struct LocalPreviewGrant" "#[derive(Debug, Clone, PartialEq, Eq)]`npub struct LocalPreviewGrant")
     test = "cargo test -p tp-core --doc --offline"; expect = "FAILED" },
  @{ id = "NC-CAP-10"; what = "LocalPreviewGrant convertible into ConsentGrant"; file = "crates\tp-core\src\preview\mod.rs"
     mutate = (Append "impl From<LocalPreviewGrant> for tp_security::ConsentGrant {`n    fn from(_grant: LocalPreviewGrant) -> Self {`n        panic!(`"negative control`")`n    }`n}")
     test = "cargo test -p tp-core --doc --offline"; expect = "FAILED" },
  @{ id = "NC-CAP-11"; what = "preview machine accepts a selection without checking generation or state"; file = "crates\tp-core\src\preview\machine.rs"
     mutate = (Replace "    pub fn selected(&mut self, generation: u32) -> Result<(), Reject> {`n        self.expect(generation, PreviewState::SelectingTarget)?;" "    pub fn selected(&mut self, generation: u32) -> Result<(), Reject> {`n        let _ = generation;")
     test = "cargo test -p tp-core --offline --test preview_machine --test preview"; expect = "FAILED" }
)

$results = @()
foreach ($nc in $controls) {
  $path = (Resolve-Path $nc.file).Path
  $original = [IO.File]::ReadAllBytes($path)
  $hash = (Get-FileHash $path -Algorithm SHA256).Hash
  $text = [Text.Encoding]::UTF8.GetString($original).Replace("`r`n", "`n")
  $outcome = "ERROR"
  try {
    [IO.File]::WriteAllText($path, (& $nc.mutate $text), (New-Object Text.UTF8Encoding $false))
    $out = (& cmd /c "$($nc.test) 2>&1") -join "`n"
    $exit = $LASTEXITCODE
    $caught = ($exit -ne 0) -and $out.Contains($nc.expect)
    $outcome = if ($caught) { "CAUGHT (guard failed as expected, exit $exit)" } else { "NOT CAUGHT (exit $exit)" }
    $tail = ($out -split "`n" | Where-Object { $_ -match 'FAILED|panicked|error\[|test result' } | Select-Object -First 6) -join " | "
  } finally {
    [IO.File]::WriteAllBytes($path, $original)
  }
  $restored = (Get-FileHash $path -Algorithm SHA256).Hash -eq $hash
  $line = "$($nc.id) [$($nc.what)] file=$($nc.file) -> $outcome; restored=$restored; evidence: $tail"
  $line | Out-File -Append -Encoding utf8 $log
  $line
  $results += [pscustomobject]@{ id = $nc.id; caught = $outcome.StartsWith("CAUGHT"); restored = $restored }
}
$lockAfter = (Get-FileHash Cargo.lock -Algorithm SHA256).Hash
"Cargo.lock unchanged: $($lockBefore -eq $lockAfter)" | Out-File -Append -Encoding utf8 $log
$verify = (& cmd /c "cargo test -p tp-test-harness --test workspace_boundaries --offline 2>&1 && cargo test -p tp-core --offline 2>&1") -join "`n"
"after restore, guards pass: $($LASTEXITCODE -eq 0) ($(([regex]::Matches($verify, 'test result: ok')).Count) ok result lines)" | Out-File -Append -Encoding utf8 $log
Get-Content $log | Select-Object -Last 3
"summary: caught=$(@($results | Where-Object caught).Count)/$($results.Count) restored=$(@($results | Where-Object restored).Count)/$($results.Count)"
