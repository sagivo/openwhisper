//! Test Gemma refinement on synthetic filler-heavy transcriptions.

use anyhow::{Context, Result};
use openwhisper_lib::config::{resolve_inference_threads, DEFAULT_REFINE_PROMPT};
use openwhisper_lib::llm_engine::LlmEngine;
use std::path::Path;
use std::time::Instant;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let llm_path = args.next().context("usage: <gemma.gguf>")?;

    println!("loading gemma...");
    let llm = LlmEngine::load(Path::new(&llm_path), resolve_inference_threads(0))?;

    let cases = [
        "um so like can you uh send him a message saying I'll be late uh by maybe like ten minutes",
        "hey so I was thinking we should you know maybe push the meeting to like Tuesday because Bob is uh out",
        "okay so the bug is um the the the user clicks the button and then like nothing happens it just kind of freezes",
        "remind me to pick up uh milk and bread tomorrow morning before like 9am",
    ];

    for raw in cases {
        let t0 = Instant::now();
        let refined = llm.refine(DEFAULT_REFINE_PROMPT, raw)?;
        let dt = t0.elapsed().as_secs_f32();
        println!("\n  RAW    ({:.2}s): {raw}", dt);
        println!("  REFINED: {refined}");
    }
    Ok(())
}
