param(
    [Parameter(Position = 0)]
    [string]$Command,

    [Parameter(Position = 1)]
    [string]$Arg1,

    [Parameter(Position = 2)]
    [string]$Arg2
)

$CommandRoot = Join-Path $PSScriptRoot "commands"

switch ($Command)
{
    "handoff" {

        $Mode = if (
        [string]::IsNullOrWhiteSpace($Arg2)
        )
        {
            "implement"
        }
        else
        {
            $Arg2
        }

        & "$CommandRoot\handoff.ps1" `
            -Ticket $Arg1 `
            -Mode $Mode
    }

    "review" {
        Write-Host ""
        Write-Host "Review command not implemented"
        Write-Host ""
    }

    "status" {
        Write-Host ""
        Write-Host "Status command not implemented"
        Write-Host ""
    }

    default {
        Write-Host ""
        Write-Host "PinkChoco Deploy Officiator"
        Write-Host ""
        Write-Host "Commands:"
        Write-Host ""
        Write-Host "  handoff <ticket>"
        Write-Host "  review <ticket>"
        Write-Host "  status"
        Write-Host ""
    }
}