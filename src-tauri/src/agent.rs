//! Tools for AI agents, shared by the MCP server (`cli mcp`) and the plain CLI.
//! Edits go through the same validator as the app. Deleting and public sharing need `confirm: true`.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::brand::BRAND;
use crate::library;
use crate::{edits, export, server, share, shorts, voice};

pub struct Agent {
    tunnel: share::Tunnel,
    port: tokio::sync::OnceCell<u16>,
}

impl Default for Agent {
    fn default() -> Self {
        Self {
            tunnel: Default::default(),
            port: tokio::sync::OnceCell::new(),
        }
    }
}

fn arg_str<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a[k].as_str().with_context(|| format!("missing \"{k}\""))
}

fn confirmed(a: &Value, what: &str) -> Result<()> {
    ensure!(
        a["confirm"].as_bool() == Some(true),
        "{what} needs \"confirm\": true. Ask the user first."
    );
    Ok(())
}

/// Tool name, description and JSON Schema for its arguments.
pub fn tools() -> Vec<(&'static str, String, Value)> {
    let id = json!({"video_id": {"type": "string", "description": "From list_videos"}});
    let with = |extra: Value, required: &[&str]| {
        let mut p = id.as_object().cloned().unwrap_or_default();
        p.extend(extra.as_object().cloned().unwrap_or_default());
        json!({"type": "object", "properties": p, "required": required})
    };
    vec![
        ("list_videos", format!("List recordings in the {} library, newest first.", BRAND.name), json!({"type": "object", "properties": {}})),
        ("get_transcript", "Transcript of a video as timed lines [start-end] in seconds.".into(), with(json!({}), &["video_id"])),
        ("get_summary", "Title, summary, chapters, notes and voice settings of a video.".into(), with(json!({}), &["video_id"])),
        (
            "apply_edit_plan",
            "Validate and apply an edit plan: {ops: [...], explanation}. Ops: cut{start,end,label}, keep_only{ranges}, \
             speed{start,end,factor 0.25-4}, set_title{title}, set_chapters{chapters:[{start,title}]}, voice{gain_db -12..12, auto}, \
             add_note{time,text}. Times are seconds. With preview: true it only describes the change. Undo with undo_edit."
                .into(),
            with(json!({"plan": {"type": "object"}, "preview": {"type": "boolean"}}), &["video_id", "plan"]),
        ),
        ("undo_edit", "Undo the last applied edit plan.".into(), with(json!({}), &["video_id"])),
        ("render", "Re-render the shareable file from the current voice settings.".into(), with(json!({}), &["video_id"])),
        ("export", "Export to Downloads. kind: mp4, captioned, srt or vtt.".into(), with(json!({"kind": {"type": "string", "enum": ["mp4", "captioned", "srt", "vtt"]}}), &["video_id", "kind"])),
        (
            "suggest_shorts",
            "Ask Claude for vertical short clips from a transcribed video. count 1-20, min and max seconds. Returns clips with ids, times, hook titles, reasons and virality scores. Nothing renders until render_shorts.".into(),
            with(json!({"count": {"type": "integer"}, "min": {"type": "number"}, "max": {"type": "number"}}), &["video_id"]),
        ),
        (
            "render_shorts",
            "Render clips from suggest_shorts as 1080x1920 MP4s with burned-in captions. framing: blur_fit, center or split.".into(),
            with(json!({"clip_ids": {"type": "array", "items": {"type": "string"}}, "framing": {"type": "string", "enum": ["blur_fit", "center", "split"]}}), &["video_id", "clip_ids"]),
        ),
        (
            "create_share_link",
            "Make a public link to the video (works while this process runs). Requires confirm: true.".into(),
            with(json!({"confirm": {"type": "boolean"}}), &["video_id", "confirm"]),
        ),
        ("stop_share", "Stop the public link.".into(), json!({"type": "object", "properties": {}})),
        (
            "delete_video",
            "Delete a recording for good. Requires confirm: true.".into(),
            with(json!({"confirm": {"type": "boolean"}}), &["video_id", "confirm"]),
        ),
    ]
}

impl Agent {
    pub async fn call(&self, name: &str, a: &Value) -> Result<Value> {
        match name {
            "list_videos" => Ok(json!(
                library::list()
                    .iter()
                    .map(|v| json!({
                        "video_id": v.id, "title": v.title, "duration": v.duration,
                        "created_at": v.created_at, "has_transcript": v.has_transcript,
                    }))
                    .collect::<Vec<_>>()
            )),
            "get_transcript" => {
                let id = arg_str(a, "video_id")?;
                let t = library::load_transcript(id)
                    .context("no transcript yet; transcribe it in the app")?;
                Ok(json!({"lines": edits::segment_transcript(&t)}))
            }
            "get_summary" => {
                let v = library::load(arg_str(a, "video_id")?)?;
                Ok(
                    json!({"title": v.title, "summary": v.summary, "chapters": v.chapters, "notes": v.notes,
                    "voice": v.voice, "duration": v.duration}),
                )
            }
            "apply_edit_plan" => {
                let id = arg_str(a, "video_id")?;
                let v = library::load(id)?;
                let plan = edits::validate(&a["plan"], v.duration)?;
                let lines = edits::describe(&plan, v.duration);
                if a["preview"].as_bool() == Some(true) {
                    return Ok(json!({"preview": lines}));
                }
                let v = edits::apply(id, &plan).await?;
                Ok(
                    json!({"applied": lines, "duration": v.duration, "title": v.title, "undo": "call undo_edit"}),
                )
            }
            "undo_edit" => {
                let v = edits::undo(arg_str(a, "video_id")?).await?;
                Ok(json!({"restored": v.title, "duration": v.duration}))
            }
            "render" => {
                let id = arg_str(a, "video_id")?;
                let v = library::load(id)?;
                voice::render(id, &v.voice).await?;
                Ok(json!({"file": library::playable_path(id)}))
            }
            "export" => {
                let (path, note) =
                    export::run(arg_str(a, "video_id")?, arg_str(a, "kind")?).await?;
                Ok(json!({"file": path, "note": note}))
            }
            "suggest_shorts" => {
                let id = arg_str(a, "video_id")?;
                let v = library::load(id)?;
                let t = library::load_transcript(id)
                    .context("no transcript yet; transcribe it in the app")?;
                let d = shorts::Limits::default();
                let limits = shorts::Limits {
                    count: a["count"].as_u64().map(|n| n as usize).unwrap_or(d.count),
                    min: a["min"].as_f64().unwrap_or(d.min),
                    max: a["max"].as_f64().unwrap_or(d.max),
                    allow_overlap: false,
                };
                let (clips, rejected) = shorts::suggest(&v, &t, &limits, &[]).await?;
                shorts::save(
                    id,
                    &shorts::ShortsFile {
                        limits: Some(limits),
                        clips: clips.clone(),
                        rendered: vec![],
                    },
                )?;
                Ok(json!({"clips": clips, "rejected": rejected}))
            }
            "render_shorts" => {
                let id = arg_str(a, "video_id")?;
                let v = library::load(id)?;
                let ids: Vec<String> = serde_json::from_value(a["clip_ids"].clone())
                    .context("clip_ids must be a list of ids")?;
                let framing: shorts::Framing =
                    serde_json::from_value(a.get("framing").cloned().unwrap_or(json!("blur_fit")))
                        .context("framing must be blur_fit, center or split")?;
                let (files, note) = shorts::render(&v, &ids, framing).await?;
                Ok(json!({"files": files, "note": note}))
            }
            "create_share_link" => {
                confirmed(a, "Sharing publicly")?;
                let v = library::load(arg_str(a, "video_id")?)?;
                let port = *self.port.get_or_try_init(server::start).await?;
                let url = self.tunnel.start(port).await?;
                Ok(
                    json!({"url": format!("{url}/v/{}", v.share_id), "note": "Works while this process is running."}),
                )
            }
            "stop_share" => {
                self.tunnel.stop().await;
                Ok(json!({"stopped": true}))
            }
            "delete_video" => {
                confirmed(a, "Deleting")?;
                let id = arg_str(a, "video_id")?;
                library::delete(id)?;
                Ok(json!({"deleted": id}))
            }
            _ => bail!("unknown tool \"{name}\""),
        }
    }
}

/// One JSON-RPC message from an MCP client; returns the reply (None for notifications).
pub async fn handle_mcp(agent: &Agent, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let result = match msg["method"].as_str().unwrap_or_default() {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": BRAND.slug, "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(
            json!({"tools": tools().into_iter().map(|(n, d, s)| json!({"name": n, "description": d, "inputSchema": s})).collect::<Vec<_>>()}),
        ),
        "tools/call" => {
            let p = &msg["params"];
            let out = agent
                .call(p["name"].as_str().unwrap_or_default(), &p["arguments"])
                .await;
            Ok(match out {
                Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}]}),
                Err(e) => {
                    json!({"content": [{"type": "text", "text": e.to_string()}], "isError": true})
                }
            })
        }
        m => Err(json!({"code": -32601, "message": format!("method not found: {m}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mcp_lists_tools_and_guards_destructive_calls() {
        let a = Agent::default();
        let r = handle_mcp(
            &a,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await
        .unwrap();
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"apply_edit_plan") && names.contains(&"delete_video"));
        let r = handle_mcp(
            &a,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "delete_video", "arguments": {"video_id": "x"}}}),
        )
        .await
        .unwrap();
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("confirm")
        );
        assert!(
            handle_mcp(
                &a,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .await
            .is_none()
        );
    }
}
