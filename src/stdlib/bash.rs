use crate::ast::BashAction;
use crate::interpreter::RuntimeError;

pub fn run_bash_dsl(action: &BashAction, argument: &str, line: usize) -> Result<(), RuntimeError> {
    match action {
        BashAction::Generate => {
            let snippet = generate_bash(argument);
            println!("# bash (generated for: {})", argument);
            println!("{}", snippet);
            println!("completed");
        }
        BashAction::Search => {
            let opts = search_tool(argument);
            println!("# search results for `{}`", argument);
            for o in opts { println!("  - {}", o); }
            println!("completed");
        }
        BashAction::WebSearch => {
            println!("# web search: {}", argument);
            println!("# (network access not performed; use run_command(\"curl ...\") if needed)");
            println!("completed");
        }
        BashAction::CompleteOrError => {
            // Heuristic status acknowledgement
            println!("completed");
        }
    }
    let _ = line;
    Ok(())
}

fn generate_bash(task: &str) -> String {
    let lower = task.to_ascii_lowercase();

    if lower.contains("oh my zsh") || lower.contains("oh-my-zsh") || lower.contains("ohmyzsh") {
        return r#"sh -c "$(curl -fsSL https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh)""#.to_string();
    }
    if lower.contains("install homebrew") || lower.contains("install brew") {
        return r#"/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#.to_string();
    }
    if lower.contains("wget") {
        return "command -v wget >/dev/null 2>&1 && echo 'wget present' || echo 'wget missing'".to_string();
    }
    if lower.contains("curl") {
        return "command -v curl >/dev/null 2>&1 && echo 'curl present' || echo 'curl missing'".to_string();
    }
    if lower.contains("nodejs") || lower.contains("node.js") {
        return "command -v node >/dev/null 2>&1 && node -v || echo 'node missing'".to_string();
    }
    if lower.contains("update") && (lower.contains("apt") || lower.contains("debian") || lower.contains("ubuntu")) {
        return "sudo apt-get update -y && sudo apt-get upgrade -y".to_string();
    }
    if lower.contains("disk") || lower.contains("space") {
        return "df -h".to_string();
    }
    if lower.contains("memory") || lower.contains("ram") {
        return if cfg!(target_os = "macos") { "vm_stat".to_string() } else { "free -h".to_string() };
    }
    if lower.contains("cpu") {
        return if cfg!(target_os = "macos") { "sysctl -n machdep.cpu.brand_string".to_string() } else { "lscpu".to_string() };
    }

    // Fallback: a comment noting the request
    format!("# TODO: implement: {}\necho 'task: {}'", task, task)
}

fn search_tool(query: &str) -> Vec<String> {
    let q = query.to_ascii_lowercase();
    if q.contains("curl") || q.contains("wget") {
        return vec!["curl (preferred on macOS/most Linux)".into(), "wget (often default on Debian/Ubuntu)".into()];
    }
    if q.contains("ohmyzsh") || q.contains("zsh") {
        return vec!["zsh".into(), "oh-my-zsh".into(), "starship".into()];
    }
    vec![format!("(no canned hits for `{}`)", query)]
}
