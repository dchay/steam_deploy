param(
    [Parameter(Mandatory = $true)]
    [string]$Ticket,

    [ValidateSet('implement','review')]
    [string]$Mode = 'implement'
)

$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'

if ($Mode -eq 'implement')
{
    $output = "handoffs/gpt/$Ticket-$timestamp.md"

    @"
TICKET
$Ticket

MODE
IMPLEMENT

PROJECT
steam_deploy

CURRENT SOURCE FILES

src/lib.rs
src/main.rs

src/models/config.rs

src/stages/validate.rs
src/stages/build.rs
src/stages/vdf.rs

REQUIRED OUTPUT

- code
- tests
- migration notes

DO NOT

- close ticket
- declare canon
- approve own work

NEXT STEP

Submit packet to GPT.
"@ | Set-Content $output

    Write-Host ""
    Write-Host "GPT packet created:"
    Write-Host $output
}
else
{
    $output = "handoffs/deepseek/$Ticket-$timestamp.md"

    @"
TICKET
$Ticket

MODE
REVIEW

PROJECT
steam_deploy

REQUIRED OUTPUT

- findings
- risks
- defects
- missing tests

DO NOT

- close ticket
- approve implementation
- declare canon

NEXT STEP

Submit packet to DeepSeek.
"@ | Set-Content $output

    Write-Host ""
    Write-Host "Review packet created:"
    Write-Host $output
}