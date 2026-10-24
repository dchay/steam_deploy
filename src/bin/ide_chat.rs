use std::{env, fs};

const PROJECT_SUMMARY: &str = ".opencode/context/project-summary.md";
const WORKFLOW: &str = ".opencode/context/workflow.md";
const CURRENT_TICKET: &str = ".officiator/active/current-ticket.md";
const FINDINGS: &str = ".officiator/active/findings.md";
const DECISIONS: &str = ".officiator/active/decisions.md";
const STATUS: &str = ".officiator/active/status.md";
const AUDIT_FILE: &str = "evidence/audits/P0-002-gemini-audit.md";
const TICKET_FILE: &str = "tickets/active/P0-002-upload-stage.md";

fn main() {
    let command = env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("help"))
        .to_ascii_lowercase();

    
    match command.as_str() {
        "status" => {
            show_file(CURRENT_TICKET, "CURRENT TICKET");
            show_file(STATUS, "STATUS");
        }
        "context" => show_file(PROJECT_SUMMARY, "PROJECT SUMMARY"),
        "workflow" => show_file(WORKFLOW, "WORKFLOW"),
        "findings" => show_file(FINDINGS, "FINDINGS"),
        "decisions" => show_file(DECISIONS, "DECISIONS"),
        "audit" => show_audit(),
        "prompt" => show_prompt(),
        "help" => show_help(),
        _ => {
            println!("Unknown command: {command}");
            show_help();
        }
    }

    

}

fn show_file(path: &str, heading: &str) {
    println!("=== {heading} ===");
    match fs::read_to_string(path) {
        Ok(contents) => {
            print!("{contents}");
            if !contents.ends_with('\n') {
                println!();
            }
        }
        Err(error) => {
            eprintln!("Error reading {path}: {error}");
        }
    }
    println!();
}

fn show_audit() {
    let ticket = match fs::read_to_string(TICKET_FILE) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ERROR: Audit or ticket file not found for P0-002.");
            eprintln!("Error reading {}: {}", TICKET_FILE, e);
            eprintln!("Expected:");
            eprintln!("evidence/audits/P0-002-gemini-audit.md");
            eprintln!("tickets/active/P0-002-upload-stage.md");
            return;
        }
    };

    
    let audit = match fs::read_to_string(AUDIT_FILE) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ERROR: Audit or ticket file not found for P0-002.");
            eprintln!("Error reading {}: {}", AUDIT_FILE, e);
            eprintln!("Expected:");
            eprintln!("evidence/audits/P0-002-gemini-audit.md");
            eprintln!("tickets/active/P0-002-upload-stage.md");
            return;
        }
    };

    println!("=== ACTIVE TICKET ===");
    print!("{ticket}");
    if !ticket.ends_with('\n') {
        println!();
    }
    println!();
    println!("=== AUDIT CONTRACT ===");
    print!("{audit}");
    if !audit.ends_with('\n') {
        println!();
    }
    println!();

    

}

fn show_prompt() {
    let ticket = match fs::read_to_string(TICKET_FILE) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ERROR: Could not generate prompt.");
            eprintln!("Error reading {}: {}", TICKET_FILE, e);
            eprintln!("Required files:");
            eprintln!("tickets/active/P0-002-upload-stage.md");
            eprintln!("evidence/audits/P0-002-gemini-audit.md");
            eprintln!(".officiator/active/status.md");
            return;
        }
    };

    
    let audit = match fs::read_to_string(AUDIT_FILE) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ERROR: Could not generate prompt.");
            eprintln!("Error reading {}: {}", AUDIT_FILE, e);
            eprintln!("Required files:");
            eprintln!("tickets/active/P0-002-upload-stage.md");
            eprintln!("evidence/audits/P0-002-gemini-audit.md");
            eprintln!(".officiator/active/status.md");
            return;
        }
    };

    let status = match fs::read_to_string(STATUS) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: Could not generate prompt.");
            eprintln!("Error reading {}: {}", STATUS, e);
            eprintln!("Required files:");
            eprintln!("tickets/active/P0-002-upload-stage.md");
            eprintln!("evidence/audits/P0-002-gemini-audit.md");
            eprintln!(".officiator/active/status.md");
            return;
        }
    };

    println!("=== IMPLEMENTATION PROMPT ===");
    println!("You are the Implementer for steam_deploy.");
    println!("Use repository state below.");
    println!("=== ACTIVE TICKET ===");
    print!("{ticket}");
    if !ticket.ends_with('\n') {
        println!();
    }
    println!("=== AUDIT CONTRACT ===");
    print!("{audit}");
    if !audit.ends_with('\n') {
        println!();
    }
    println!("=== CURRENT STATUS ===");
    print!("{status}");
    if !status.ends_with('\n') {
        println!();
    }
    println!("=== IMPLEMENTER INSTRUCTIONS ===");
    println!("Use the active ticket and audit contract.");
    println!("Provide complete implementation.");
    println!("Do not generate TODOs.");
    println!("Do not generate placeholder code.");
    println!("Ensure code compiles.");
    println!("Output complete source files only.");
    println!("Preserve original markdown and formatting from repository files.");
    println!("Do not parse or summarize file contents.");
    println!("Simply assemble the worker prompt from repository artifacts.");

    

}

fn show_help() {
    println!("IDE Chat MVP");
    println!();
    println!("Available commands:");
    println!("  status     Show current ticket and status");
    println!("  context    Show project summary");
    println!("  workflow   Show workflow");
    println!("  findings   Show findings");
    println!("  decisions  Show decisions");
    println!("  audit      Show P0-002 ticket and audit contract");
    println!("  prompt     Generate worker implementation prompt");
    println!("  help       Show this help");
}