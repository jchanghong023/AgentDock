# Deterministic local rendering load; no project files or external services used.
$esc = [char]27
Start-Sleep -Seconds 2
for ($frame = 0; $frame -lt 240; $frame++) {
    $rows = for ($line = 0; $line -lt 30; $line++) {
        "{0}[{1}m{2:D4}  {3:D2}  {4}{0}[0m" -f $esc, (32 + $line % 5), $frame, $line, ('abcdef0123456789' * 5)
    }
    [Console]::Write("${esc}[H" + ($rows -join "`r`n"))
    Start-Sleep -Milliseconds 33
}
[Console]::Write("`r`nDEVHUB_RENDER_LOAD_DONE`r`n")
Start-Sleep -Seconds 30
