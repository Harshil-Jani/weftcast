//! Command line and MCP server for AI agents.
//!   cli mcp                         stdio MCP server (add with: claude mcp add <slug> -- <path-to-cli> mcp)
//!   cli list | transcript ID | summary ID
//!   cli plan ID "remove the first 5 seconds"   ask Claude for an edit plan and print it
//!   cli apply ID plan.json [--preview]         validate and apply (or only describe) a plan
//!   cli undo ID | export ID KIND | share ID --yes | delete ID --yes
use app_lib::agent::{Agent, handle_mcp};
use app_lib::{edits, library};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args).await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(args: &[String]) -> anyhow::Result<()> {
    let agent = Agent::default();
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let yes = args.iter().any(|x| x == "--yes");
    let out = match a(0).as_str() {
        "mcp" => return mcp(&agent).await,
        "list" => agent.call("list_videos", &json!({})).await?,
        "transcript" => {
            agent
                .call("get_transcript", &json!({"video_id": a(1)}))
                .await?
        }
        "summary" => {
            agent
                .call("get_summary", &json!({"video_id": a(1)}))
                .await?
        }
        "plan" => {
            let v = library::load(&a(1))?;
            let plan = edits::plan_from_prompt(&v, library::load_transcript(&a(1)).as_ref(), &a(2))
                .await?;
            json!({"plan": plan, "preview": edits::describe(&plan, v.duration)})
        }
        "apply" => {
            let plan: Value = serde_json::from_str(&std::fs::read_to_string(a(2))?)?;
            let plan = plan.get("plan").cloned().unwrap_or(plan);
            let preview = args.iter().any(|x| x == "--preview");
            agent
                .call(
                    "apply_edit_plan",
                    &json!({"video_id": a(1), "plan": plan, "preview": preview}),
                )
                .await?
        }
        "undo" => agent.call("undo_edit", &json!({"video_id": a(1)})).await?,
        "export" => {
            agent
                .call("export", &json!({"video_id": a(1), "kind": a(2)}))
                .await?
        }
        "delete" => {
            agent
                .call("delete_video", &json!({"video_id": a(1), "confirm": yes}))
                .await?
        }
        "share" => {
            let r = agent
                .call(
                    "create_share_link",
                    &json!({"video_id": a(1), "confirm": yes}),
                )
                .await?;
            println!(
                "{}\nPress Ctrl+C to stop sharing.",
                r["url"].as_str().unwrap_or_default()
            );
            tokio::signal::ctrl_c().await?;
            return Ok(());
        }
        _ => {
            eprintln!(
                "usage: cli mcp | list | transcript ID | summary ID | plan ID \"request\" | apply ID plan.json [--preview] | undo ID | export ID mp4|captioned|srt|vtt | share ID --yes | delete ID --yes"
            );
            std::process::exit(2);
        }
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

/// Line-delimited JSON-RPC over stdin/stdout.
async fn mcp(agent: &Agent) -> anyhow::Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle_mcp(agent, &msg).await,
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}),
            ),
        };
        if let Some(r) = reply {
            stdout.write_all(format!("{r}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
