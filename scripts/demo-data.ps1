<#
.SYNOPSIS
  Generates a fake Claude config dir for SessionStation screenshots and demos.

.DESCRIPTION
  Creates <Dir> (default %TEMP%\sessionstation-demo) with sessions/, projects/
  (live + closed transcripts, subagents, a workflow), starts 5 harmless dummy
  processes so the live-session liveness check passes, and creates the demo
  folders under C:\Demo. Everything is made up. -Stop undoes all of it.

  Launch the app against it:
    $env:CLAUDE_CONFIG_DIR = "<Dir>"; & "<repo>\target\debug\session-station.exe"

.PARAMETER Dir
  Demo config dir. Only this dir is ever wiped (and only if this script made it).

.PARAMETER Stop
  Kill the recorded dummy processes, delete <Dir> and the C:\Demo folders created.
#>
param(
    [string]$Dir = (Join-Path $env:TEMP 'sessionstation-demo'),
    [switch]$Stop
)
$ErrorActionPreference = 'Stop'
$Dir = [System.IO.Path]::GetFullPath($Dir)
$Marker = Join-Path $Dir '.sessionstation-demo'
$Manifest = Join-Path $Dir 'demo-manifest.json'
$DemoRoot = 'C:\Demo'
$Utf8 = New-Object System.Text.UTF8Encoding($false)

function Stop-Demo {
    if (-not (Test-Path -LiteralPath $Dir)) { Write-Host "Nothing to stop ($Dir does not exist)."; return }
    if (-not (Test-Path -LiteralPath $Marker)) { throw "$Dir was not created by demo-data.ps1 (no marker); refusing to touch it." }
    if (Test-Path -LiteralPath $Manifest) {
        $m = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
        foreach ($p in @($m.processes)) {
            $proc = Get-Process -Id $p.pid -ErrorAction SilentlyContinue
            # Only kill when the creation time still matches (guards against PID reuse).
            if ($proc -and ([string]$proc.StartTime.ToFileTimeUtc() -eq [string]$p.procStart)) {
                Stop-Process -Id $p.pid -Force -ErrorAction SilentlyContinue
                Write-Host "Killed dummy process $($p.pid)"
            }
        }
        foreach ($d in @($m.createdDirs) | Sort-Object -Descending) {
            if ($d -and $d.StartsWith($DemoRoot, [System.StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $d)) {
                Remove-Item -LiteralPath $d -Recurse -Force
                Write-Host "Removed $d"
            }
        }
    }
    Remove-Item -LiteralPath $Dir -Recurse -Force
    Write-Host "Removed $Dir"
}

if ($Stop) { Stop-Demo; return }

# Re-running replaces the previous demo (only if it is ours).
if (Test-Path -LiteralPath $Dir) {
    if (-not (Test-Path -LiteralPath $Marker) -and (Get-ChildItem -LiteralPath $Dir -Force | Select-Object -First 1)) {
        throw "$Dir exists and is not a demo dir (no marker); refusing to wipe it."
    }
    if (Test-Path -LiteralPath $Marker) { Stop-Demo }
}
New-Item -ItemType Directory -Path $Dir -Force | Out-Null
Set-Content -LiteralPath $Marker -Value 'created by scripts/demo-data.ps1' -Encoding ASCII
$SessionsDir = Join-Path $Dir 'sessions'
$ProjectsDir = Join-Path $Dir 'projects'
New-Item -ItemType Directory -Path $SessionsDir, $ProjectsDir -Force | Out-Null

$Rng = New-Object System.Random 20261102
$Now = [DateTime]::UtcNow
function Rnd([double]$a, [double]$b) { $a + $Rng.NextDouble() * ($b - $a) }
function RndInt([int]$a, [int]$b) { $Rng.Next($a, $b + 1) }
function Pick($arr) { $arr[$Rng.Next(0, @($arr).Count)] }
function NewId { [guid]::NewGuid().ToString() }
function Hex([int]$n) { -join (1..$n | ForEach-Object { '{0:x}' -f $Rng.Next(0, 16) }) }
function Iso([DateTime]$t) { $t.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'") }
function Ms([DateTime]$t) { [DateTimeOffset]::new($t.ToUniversalTime()).ToUnixTimeMilliseconds() }
function J($o) { $o | ConvertTo-Json -Compress -Depth 12 }
function ProjDirName([string]$cwd) { ($cwd.ToCharArray() | ForEach-Object { if ($_ -match '[A-Za-z0-9]') { $_ } else { '-' } }) -join '' }
function Write-Lines([string]$path, $lines, [DateTime]$mtime) {
    New-Item -ItemType Directory -Path (Split-Path $path) -Force | Out-Null
    [System.IO.File]::WriteAllText($path, (($lines -join "`n") + "`n"), $Utf8)
    (Get-Item -LiteralPath $path).LastWriteTimeUtc = $mtime
}

# $/M tokens: input, output, cache read, 5m cache write, 1h cache write (mirrors core/src/pricing.rs).
$Price = @{
    'claude-opus-5-5'   = @(4.0, 20.0, 0.20, 5.0, 8.0)
    'claude-sonnet-5-5' = @(2.0, 10.0, 0.20, 2.5, 4.0)
    'claude-haiku-5-5'  = @(0.10, 0.50, 0.01, 0.125, 0.2)
}
$script:CostLog = @{}

# ---- content pools (all made up) -------------------------------------------
$Files = @('src\cart.ts', 'src\checkout.ts', 'src\routes\orders.ts', 'src\db\schema.sql', 'lib\router.go', 'lib\auth.go',
    'pipeline\ingest.py', 'pipeline\transform.py', 'app\HomeScreen.kt', 'app\Network.kt', 'docs\guide.md', 'docs\config.md',
    'tests\cart.test.ts', 'tests\router_test.go', 'package.json', 'README.md')
$Commands = @('npm test', 'npm run build', 'go test ./...', 'pytest -q', 'git status', 'git diff --stat', 'dir src', 'npm run lint',
    'gradle assembleDebug', 'python -m pipeline.ingest --dry-run', 'git log --oneline -5')
$Patterns = @('TODO', 'handleError', 'useCart', 'func Route', 'def transform', 'class .*Repository', 'deprecated', 'migrate')
$Say = @('Looking at the current structure first.', 'The failing test points at the cache key; checking it.', 'Applying the change and re-running the checks.',
    'This touches three call sites; updating them together.', 'Found the cause: a stale value is read before the refresh.',
    'Splitting the helper out so it can be tested on its own.', 'The refactor keeps the public API unchanged.',
    'Writing the migration so it can be rolled back.', 'Build is green; moving on to the next module.')
$Done = @('Done. The refactor is in place and the tests pass.', 'The migration is written and verified against the sample data.',
    'All checks pass; summary of the changes is above.', 'Fixed. I also added a regression test for it.')
$Prompts = @('Refactor the cart module so pricing and discounts are separate.', 'Write a migration that adds the new order status column.',
    'Why does the nightly job fail on empty batches?', 'Add retry with backoff to the HTTP client.', 'Clean up the router and add tests.',
    'Speed up the build; it takes too long on CI.', 'Document the configuration options.', 'Replace the deprecated API calls.')
$AgentTypes = @('Explore', 'general-purpose', 'reviewer', 'verifier', 'planner')
$AgentDescs = @('Survey module layout', 'Find usages of the cart API', 'Review the router change', 'Run the test suite',
    'Check migration safety', 'Summarize failing tests', 'Draft the plan for the refactor', 'Verify build output',
    'Scan docs for stale options', 'Trace the retry path')

function New-ToolInput([string]$name, [string]$cwd) {
    switch ($name) {
        'Read' { @{ file_path = Join-Path $cwd (Pick $Files) } }
        'Edit' { @{ file_path = Join-Path $cwd (Pick $Files); old_string = 'oldValue'; new_string = 'newValue' } }
        'Bash' { @{ command = Pick $Commands } }
        default { @{ pattern = Pick $Patterns } }
    }
}

# Evenly spread, slightly jittered, ascending timestamps.
function Get-Times([int]$n, [DateTime]$from, [DateTime]$to) {
    $span = ($to - $from).TotalSeconds
    for ($i = 0; $i -lt $n; $i++) { $from.AddSeconds($span * ($i + (Rnd 0 0.6)) / $n) }
}

# Builds transcript lines. $Times: ascending turn timestamps. Context fill ramps CtxFrom->CtxTo of 1M.
function New-Transcript {
    param([string]$SessionId, [string]$Cwd, [string]$Branch, [string]$Model, [string]$Title, [DateTime[]]$Times,
        [double]$CtxFrom, [double]$CtxTo, [string]$End = 'text', [bool]$Requested = $false, [double]$Heavy = 1.0)
    $lines = New-Object System.Collections.Generic.List[string]
    $p = $Price[$Model]; $cost = 0.0
    $base = @{ sessionId = $SessionId; cwd = $Cwd; gitBranch = $Branch; version = '9.9.9' }
    $lines.Add((J ([ordered]@{ type = 'custom-title'; customTitle = $Title; sessionId = $SessionId })))
    $n = $Times.Count
    for ($i = 0; $i -lt $n; $i++) {
        $t = $Times[$i]
        $last = ($i -eq $n - 1)
        if ($i % 8 -eq 0) {
            $lines.Add((J ([ordered]@{ type = 'user'; timestamp = Iso $t.AddSeconds(-6); cwd = $Cwd; gitBranch = $Branch; sessionId = $SessionId
                        message = [ordered]@{ role = 'user'; content = (Pick $Prompts) } })))
        }
        $ctx = [int](1000000 * ($CtxFrom + ($CtxTo - $CtxFrom) * $i / [Math]::Max(1, $n - 1)))
        $create = [int]((Rnd 2000 15000) * $Heavy); $in = RndInt 3 400; $out = [int]((Rnd 400 3500) * $Heavy)
        $read = [Math]::Max(0, $ctx - $create - $in)
        $c1h = [int]($create * 0.3); $c5 = $create - $c1h
        $cost += ($in * $p[0] + $out * $p[1] + $read * $p[2] + $c5 * $p[3] + $c1h * $p[4]) / 1e6
        $textOnly = ($last -and $End -eq 'text')
        $content = @([ordered]@{ type = 'text'; text = (Pick $Say) })
        $tid = $null
        if (-not $textOnly) {
            $tid = 'toolu_' + (Hex 20)
            $tool = Pick @('Read', 'Read', 'Edit', 'Bash', 'Grep', 'Edit', 'Bash')
            $content += [ordered]@{ type = 'tool_use'; id = $tid; name = $tool; input = (New-ToolInput $tool $Cwd) }
        } else { $content = @([ordered]@{ type = 'text'; text = (Pick $Done) }) }
        $msg = [ordered]@{ id = 'msg_' + (Hex 24); type = 'message'; role = 'assistant'; model = $Model; content = $content
            usage = [ordered]@{ input_tokens = $in; output_tokens = $out; cache_read_input_tokens = $read; cache_creation_input_tokens = $create
                cache_creation = [ordered]@{ ephemeral_5m_input_tokens = $c5; ephemeral_1h_input_tokens = $c1h } } }
        $a = [ordered]@{ type = 'assistant'; timestamp = Iso $t; cwd = $Cwd; gitBranch = $Branch; sessionId = $SessionId; message = $msg }
        if ($Requested) { $a['requestedModel'] = "$Model[1m]" }
        $lines.Add((J $a))
        # Open tool call on the last line when $End = 'tool' (running / awaiting approval).
        if ($tid -and -not ($last -and $End -eq 'tool')) {
            $err = ($tool -eq 'Bash' -and $Rng.NextDouble() -lt 0.3) -or ($Rng.NextDouble() -lt 0.06)
            $res = [ordered]@{ type = 'tool_result'; tool_use_id = $tid; content = $(if ($err) { 'Error: exit code 1' } else { 'ok' }) }
            if ($err) { $res['is_error'] = $true }
            $lines.Add((J ([ordered]@{ type = 'user'; timestamp = Iso $t.AddSeconds((Rnd 1 5)); cwd = $Cwd; gitBranch = $Branch; sessionId = $SessionId
                            message = [ordered]@{ role = 'user'; content = @($res) } })))
        }
    }
    $script:CostLog[$SessionId] = $cost
    return , $lines
}

# One subagent: jsonl + meta.json. $Running: unanswered tool call, fresh mtime.
function New-Agent {
    param([string]$SubDir, [string]$Cwd, [string]$Model, [DateTime]$Start, [DateTime]$End, [bool]$Running,
        [string]$Type, [string]$Desc, [string]$Parent = $null, [string]$Phase = $null, [int]$Depth = 1)
    $id = 'a' + (Hex 16)
    $n = RndInt 3 6
    $times = @(Get-Times $n $Start $End)
    $endKind = if ($Running) { 'tool' } else { 'text' }
    $lines = New-Transcript -SessionId (NewId) -Cwd $Cwd -Branch 'main' -Model $Model -Title 'x' -Times $times -CtxFrom 0.03 -CtxTo (Rnd 0.06 0.15) -End $endKind -Heavy 0.6
    $lines.RemoveAt(0)   # no title line in agent files
    $meta = [ordered]@{ agentType = $Type; description = $Desc; spawnDepth = $Depth }
    if ($Parent) { $meta['parentAgentId'] = $Parent }
    if ($Phase) { $meta['workflowPhase'] = $Phase }
    if (-not $Running) { $meta['finished'] = $true }
    $mt = if ($Running) { $Now } else { $times[-1] }
    Write-Lines (Join-Path $SubDir "agent-$id.jsonl") $lines $mt
    [System.IO.File]::WriteAllText((Join-Path $SubDir "agent-$id.meta.json"), (J $meta), $Utf8)
    (Get-Item -LiteralPath (Join-Path $SubDir "agent-$id.meta.json")).LastWriteTimeUtc = $mt
    return $id
}

# ---- demo folders under C:\Demo -------------------------------------------
$createdDirs = New-Object System.Collections.Generic.List[string]
function Ensure-Folder([string]$path) {
    if (-not (Test-Path -LiteralPath $path)) {
        if (-not (Test-Path -LiteralPath $DemoRoot)) { New-Item -ItemType Directory -Path $DemoRoot | Out-Null; $createdDirs.Add($DemoRoot) }
        New-Item -ItemType Directory -Path $path | Out-Null
        $createdDirs.Add($path)
    }
}
$Projects = @('web-shop', 'api-gateway', 'data-pipeline', 'mobile-app', 'docs-site', 'billing-service')
foreach ($p in $Projects) { Ensure-Folder (Join-Path $DemoRoot $p) }

# ---- dummy processes -------------------------------------------------------
$procs = @()
for ($i = 0; $i -lt 5; $i++) {
    $pr = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile', '-WindowStyle', 'Hidden', '-Command', 'Start-Sleep 86400' -WindowStyle Hidden -PassThru
    $procs += [pscustomobject]@{ pid = $pr.Id; procStart = [string][System.Diagnostics.Process]::GetProcessById($pr.Id).StartTime.ToFileTimeUtc() }
}
# Persist early so -Stop works even if generation fails below.
$manifestObj = [ordered]@{ processes = $procs; createdDirs = @($createdDirs) }
[System.IO.File]::WriteAllText($Manifest, (J $manifestObj), $Utf8)

# ---- live sessions ---------------------------------------------------------
# status, waitingFor, model, turns, ctx% (end), started h ago, last activity (minutes ago), old turns (days ago), end kind, agents
$Live = @(
    @{ Name = 'web-shop';      Status = 'busy';    Model = 'claude-opus-5-5';   Turns = 70; Ctx = 0.62; StartH = 5.0; LastMin = 0.2; Old = 22; End = 'tool'; Branch = 'feat/cart-refactor'; Req = $true;  Agents = 'big'; Heavy = 3.6 }
    @{ Name = 'api-gateway';   Status = 'waiting'; Model = 'claude-sonnet-5-5'; Turns = 38; Ctx = 0.41; StartH = 3.0; LastMin = 4;   Old = 10; End = 'tool'; Branch = 'main';              Req = $false; Agents = 2; Heavy = 2.0 }
    @{ Name = 'data-pipeline'; Status = 'idle';    Model = 'claude-opus-5-5';   Turns = 55; Ctx = 0.70; StartH = 6.0; LastMin = 25;  Old = 30; End = 'text'; Branch = 'fix/empty-batch';    Req = $false; Agents = 1; Heavy = 3.0 }
    @{ Name = 'mobile-app';    Status = 'shell';   Model = 'claude-haiku-5-5';  Turns = 30; Ctx = 0.10; StartH = 2.0; LastMin = 8;   Old = 0;  End = 'text'; Branch = 'main';              Req = $false; Agents = 0 }
    @{ Name = 'docs-site';     Status = 'busy';    Model = 'claude-sonnet-5-5'; Turns = 26; Ctx = 0.18; StartH = 1.2; LastMin = 0.1; Old = 6;  End = 'tool'; Branch = 'docs/config-page';   Req = $false; Agents = 3 }
)
$wfPhases = @('Plan', 'Implement', 'Review', 'Verify')
for ($i = 0; $i -lt $Live.Count; $i++) {
    $s = $Live[$i]; $pr = $procs[$i]
    $cwd = Join-Path $DemoRoot $s.Name
    $sid = NewId
    $startT = $Now.AddHours(-$s.StartH); $lastT = $Now.AddMinutes(-$s.LastMin)
    $times = @()
    if ($s.Old -gt 0) {
        $d0 = $Now.AddDays(-(RndInt 2 4)).Date.AddHours(9)
        $times += Get-Times $s.Old $d0 $d0.AddHours(6)
    }
    $times += Get-Times $s.Turns $startT $lastT
    $times = @($times | Sort-Object)
    $total = $times.Count
    $lines = New-Transcript -SessionId $sid -Cwd $cwd -Branch $s.Branch -Model $s.Model -Title $s.Name -Times $times -CtxFrom 0.04 -CtxTo $s.Ctx -End $s.End -Requested $s.Req -Heavy $(if ($s.Heavy) { $s.Heavy } else { 1.0 })
    $projDir = Join-Path $ProjectsDir (ProjDirName $cwd)
    $mtime = if ($s.End -eq 'tool' -and $s.Status -eq 'busy') { $Now } else { $lastT }
    Write-Lines (Join-Path $projDir "$sid.jsonl") $lines $mtime

    # subagents
    $sub = Join-Path $projDir "$sid\subagents"
    if ($s.Agents -eq 'big') {
        New-Item -ItemType Directory -Path $sub -Force | Out-Null
        $ids = @()
        # 6 plain agents: 4 finished (two are children), 2 running
        $a1 = New-Agent $sub $cwd 'claude-sonnet-5-5' $Now.AddMinutes(-80) $Now.AddMinutes(-70) $false 'Explore' 'Survey module layout'
        $a2 = New-Agent $sub $cwd 'claude-sonnet-5-5' $Now.AddMinutes(-65) $Now.AddMinutes(-50) $false 'planner' 'Draft the plan for the refactor'
        $null = New-Agent $sub $cwd 'claude-haiku-5-5' $Now.AddMinutes(-60) $Now.AddMinutes(-55) $false 'Explore' 'Find usages of the cart API' $a2 $null 2
        $null = New-Agent $sub $cwd 'claude-haiku-5-5' $Now.AddMinutes(-58) $Now.AddMinutes(-52) $false 'Explore' 'Scan docs for stale options' $a2 $null 2
        $r1 = New-Agent $sub $cwd 'claude-opus-5-5' $Now.AddMinutes(-9) $Now.AddSeconds(-20) $true 'reviewer' 'Review the router change'
        $null = New-Agent $sub $cwd 'claude-sonnet-5-5' $Now.AddMinutes(-4) $Now.AddSeconds(-8) $true 'verifier' 'Run the test suite' $r1 $null 2
        # workflow with phases (6 finished agents)
        $wf = Join-Path $sub ("workflows\wf_" + (Hex 12))
        New-Item -ItemType Directory -Path $wf -Force | Out-Null
        $w = $Now.AddMinutes(-48)
        foreach ($ph in @(@('Plan', 1), @('Implement', 2), @('Review', 2), @('Verify', 1))) {
            for ($k = 0; $k -lt $ph[1]; $k++) {
                $null = New-Agent $wf $cwd (Pick @('claude-sonnet-5-5', 'claude-opus-5-5')) $w $w.AddMinutes(5) $false (Pick $AgentTypes) (Pick $AgentDescs) $null $ph[0]
            }
            $w = $w.AddMinutes(7)
        }
    } elseif ($s.Agents -gt 0) {
        New-Item -ItemType Directory -Path $sub -Force | Out-Null
        for ($k = 0; $k -lt $s.Agents; $k++) {
            $running = ($s.Status -eq 'busy' -and $k -eq 0)
            $st = $Now.AddMinutes(-(RndInt 20 50)); $en = if ($running) { $Now.AddSeconds(-10) } else { $st.AddMinutes((RndInt 4 12)) }
            $null = New-Agent $sub $cwd (Pick @('claude-sonnet-5-5', 'claude-haiku-5-5')) $st $en $running (Pick $AgentTypes) (Pick $AgentDescs)
        }
    }

    # registry
    $reg = [ordered]@{ pid = [int]$pr.pid; sessionId = $sid; cwd = $cwd; startedAt = (Ms $startT); procStart = $pr.procStart
        version = '9.9.9'; kind = 'interactive'; entrypoint = 'cli'; name = $s.Name; status = $s.Status
        updatedAt = (Ms $lastT); statusUpdatedAt = (Ms $lastT) }
    if ($s.Status -eq 'waiting') { $reg['waitingFor'] = 'input needed' }
    [System.IO.File]::WriteAllText((Join-Path $SessionsDir "$($pr.pid).json"), (J $reg), $Utf8)
}

# ---- closed sessions (~25 over 30 days, 6 projects) ------------------------
$Closed = 0
$Models = @('claude-opus-5-5', 'claude-sonnet-5-5', 'claude-sonnet-5-5', 'claude-haiku-5-5', 'claude-opus-5-5')
$Titles = @('Refactor the cart module', 'Migration to the new order schema', 'Fix flaky router test', 'Add retry to HTTP client',
    'Speed up CI build', 'Document config options', 'Refactor auth middleware', 'Database migration dry run',
    'Nightly job debugging', 'Replace deprecated API', 'Cleanup unused assets', 'Write release notes')
for ($i = 0; $i -lt 25; $i++) {
    $proj = $Projects[$i % $Projects.Count]
    $cwd = Join-Path $DemoRoot $proj
    $sid = NewId
    $daysAgo = [Math]::Min(29.5, 0.6 + $i * 1.15 + (Rnd 0 0.5))
    $startT = $Now.AddDays(-$daysAgo).Date.AddHours((Rnd 8 17))
    $turns = RndInt 8 55
    $endT = $startT.AddMinutes((RndInt 25 180))
    if ($endT -gt $Now.AddMinutes(-30)) { $endT = $Now.AddMinutes(-30); $startT = $endT.AddMinutes(-60) }
    $model = $Models[$i % $Models.Count]
    $heavy = if ($model -eq 'claude-haiku-5-5') { 1.0 } else { Rnd 1.0 3.0 }
    $times = @(Get-Times $turns $startT $endT)
    $title = $Titles[$i % $Titles.Count]
    $lines = New-Transcript -SessionId $sid -Cwd $cwd -Branch (Pick @('main', 'feat/refactor', 'fix/bug')) -Model $model -Title $title -Times $times -CtxFrom 0.04 -CtxTo (Rnd 0.2 0.65) -End 'text' -Heavy $heavy
    $projDir = Join-Path $ProjectsDir (ProjDirName $cwd)
    Write-Lines (Join-Path $projDir "$sid.jsonl") $lines $times[-1]
    $na = Pick @(0, 0, 1, 2, 3)
    if ($na -gt 0) {
        $sub = Join-Path $projDir "$sid\subagents"
        New-Item -ItemType Directory -Path $sub -Force | Out-Null
        for ($k = 0; $k -lt $na; $k++) {
            $st = $times[[Math]::Min($times.Count - 1, 2 + $k)]
            $null = New-Agent $sub $cwd (Pick @('claude-sonnet-5-5', 'claude-haiku-5-5')) $st $st.AddMinutes(6) $false (Pick $AgentTypes) (Pick $AgentDescs)
        }
    }
    $Closed++
}

$sumLive = 0.0; foreach ($v in $script:CostLog.Values) { $sumLive += $v }
Write-Host ""
Write-Host "Demo data ready: $Dir"
Write-Host ("  5 live sessions (PIDs {0}), $Closed closed sessions, ~`${1:N0} main-transcript cost in total" -f (($procs | ForEach-Object { $_.pid }) -join ', '), $sumLive)
Write-Host ""
Write-Host "Launch the app against it:"
$repo = Split-Path -Parent $PSScriptRoot
Write-Host ('  $env:CLAUDE_CONFIG_DIR="{0}"; & "{1}\target\debug\session-station.exe"' -f $Dir, $repo)
Write-Host ""
$stopCmd = '.\scripts\demo-data.ps1 -Stop'
if ($PSBoundParameters.ContainsKey('Dir')) { $stopCmd += ' -Dir "' + $Dir + '"' }
Write-Host "Clean up with:  $stopCmd"
