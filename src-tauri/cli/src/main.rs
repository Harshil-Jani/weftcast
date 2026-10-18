//! Command line and MCP server for AI agents.
//!   cli mcp                         stdio MCP server (add with: claude mcp add <slug> -- <path-to-cli> mcp)
//!   cli list | transcript ID | summary ID
//!   cli plan ID "remove the first 5 seconds"   ask Claude for an edit plan and print it
//!   cli apply ID plan.json [--preview]         validate and apply (or only describe) a plan
//!   cli undo ID | export ID KIND | share ID --yes | delete ID --yes
//!   cli record SECONDS | import FILE_OR_YOUTUBE_URL | transcribe ID
//!   cli shorts ID [COUNT MIN MAX] | render-shorts ID FRAMING CLIP_ID...
use app_lib::agent::{Agent, handle_mcp};
use app_lib::{capture, edits, importer, library, transcribe};
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
        "record" => {
            let secs: u64 = a(1).parse().unwrap_or(10);
            let devs = capture::devices().await?;
            let rec = capture::Capture::default();
            let screen = devs.screens.first().map(|d| d.index.clone()).ok_or_else(|| anyhow::anyhow!("no screen to record"))?;
            rec.start(capture::StartOptions { screen, mic: devs.mics.first().map(|m| m.index.clone()) }).await?;
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            let v = rec.stop().await?;
            json!({"video_id": v.id, "duration": v.duration})
        }
        "import" => {
            let src = a(1);
            let v = if importer::is_youtube(&src) {
                importer::youtube(&src, |p| eprint!("\rdownloading {p}%   ")).await?
            } else {
                importer::file(std::path::Path::new(&src)).await?
            };
            json!({"video_id": v.id, "title": v.title, "duration": v.duration, "source_url": v.source_url})
        }
        "transcribe" => {
            let id = a(1);
            let t = transcribe::transcribe(&id, std::sync::Arc::new(|stage: &str, d: u64, t: u64| {
                if let Some(p) = (d * 100).checked_div(t) {
                    eprint!("\r{stage} {p}%   ");
                }
            }))
            .await?;
            let mut v = library::load(&id)?;
            v.has_transcript = !t.words.is_empty();
            library::save(&v)?;
            json!({"video_id": id, "words": t.words.len()})
        }
        "shorts" => {
            let n = |i: usize, d: f64| args.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(d);
            agent.call("suggest_shorts", &json!({"video_id": a(1), "count": n(2, 5.0) as u64, "min": n(3, 15.0), "max": n(4, 60.0)})).await?
        }
        "render-shorts" => {
            agent.call("render_shorts", &json!({"video_id": a(1), "framing": a(2), "clip_ids": args.get(3..).unwrap_or_default()})).await?
        }
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
                "usage: cli mcp | list | transcript ID | summary ID | plan ID \"request\" | apply ID plan.json [--preview] | undo ID | export ID mp4|captioned|srt|vtt | share ID --yes | delete ID --yes | record SECONDS | import FILE_OR_URL | transcribe ID | shorts ID [COUNT MIN MAX] | render-shorts ID FRAMING CLIP_ID..."
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
