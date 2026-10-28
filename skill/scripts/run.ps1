# 技能入口（Windows）：呼叫同目錄或 PATH 上的 twiv.exe
# 用法：.\run.ps1 check AB12345678 --period 11504
param([Parameter(ValueFromRemainingArguments = $true)]$Args2)

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
foreach ($cand in @("$here\twiv.exe", "$here\twiv")) {
    if (Test-Path $cand) { & $cand @Args2; exit $LASTEXITCODE }
}
$cmd = Get-Command twiv -ErrorAction SilentlyContinue
if ($cmd) { & $cmd.Source @Args2; exit $LASTEXITCODE }

Write-Error "找不到 twiv.exe。請把執行檔放到 skill\scripts\ 或加入 PATH。"
exit 127
