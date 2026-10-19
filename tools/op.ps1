param(
    [Parameter(Position = 0)]
    [string]$Command,

    [Parameter(Position = 1)]
    [string]$Arg1,

    [Parameter(Position = 2)]
    [string]$Arg2
)

$CommandRoot = Join-Path $PSScriptRoot "commands"

function Show-Usage {
    Write-Host ""
    Write-Host "PinkChoco Deploy Officiator"
    Write-Host ""
    Write-Host "Usage:"
    Write-Host ""
    Write-Host "  .\tools\op.ps1 status"
    Write-Host "  .\tools\op.ps1 handoff <ticket>"
    Write-Host "  .\tools\op.ps1 handoff <ticket> review"
    Write-Host "  .\tools\op.ps1 review <ticket>"
    Write-Host ""
}

if ([string]::IsNullOrWhiteSpace($Command))
{
    Write-Host ""
    Write-Host "ERROR: Missing command."
    Show-Usage
    exit 1
}

switch ($Command.ToLower())
{
    "handoff" {

        if ([string]::IsNullOrWhiteSpace($Arg1))
        {
            Write-Host ""
            Write-Host "ERROR: Missing ticket."
            Write-Host ""
            Write-Host "Example:"
            Write-Host "  .\tools\op.ps1 handoff P0-002"
            Write-Host ""
            exit 1
        }

        $Mode = if ([string]::IsNullOrWhiteSpace($Arg2))
        {
            "implement"
        }
        else
        {
            $Arg2.ToLower()
        }

        if ($Mode -notin @("implement","review"))
        {
            Write-Host ""
            Write-Host "ERROR: Invalid mode '$Mode'"
            Write-Host ""
            Write-Host "Valid modes:"
            Write-Host "  implement"
            Write-Host "  review"
            Write-Host ""
            exit 1
        }

        & "$CommandRoot\handoff.ps1" `
            -Ticket $Arg1 `
            -Mode $Mode

        break
    }

    "review" {

        if ([string]::IsNullOrWhiteSpace($Arg1))
        {
            Write-Host ""
            Write-Host "ERROR: Missing ticket."
            Write-Host ""
            Write-Host "Example:"
            Write-Host "  .\tools\op.ps1 review P0-002"
            Write-Host ""
            exit 1
        }

        & "$CommandRoot\review.ps1" `
            -Ticket $Arg1

        break
    }

    "status" {

        & "$CommandRoot\status.ps1"

        break
    }

    default {

        Write-Host ""
        Write-Host "ERROR: Unknown command '$Command'"
        Show-Usage

        exit 1
    }
}