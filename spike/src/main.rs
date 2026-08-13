//! **Does `tract` run this ONNX graph, and what is in it?**
//!
//! The one survivor of two spikes, kept because it is the tool you point at the
//! *next* candidate. It answered that question for comic-text-detector in about
//! ten minutes (`docs/notes.md`, "comic-text-detector through `tract`") and the
//! answer decided a whole phase: seventeen operators, all supported, opset 11,
//! and no `NonMaxSuppression` in the file at all.
//!
//! Outside the workspace on purpose, so its dependency and its lint standards
//! cannot reach the real crates or CI.
//!
//! Three stages, reported separately, because they fail for different reasons:
//!
//!   1. protobuf parse       — a format or version problem
//!   2. graph construction   — an operator tract has no rule for, named
//!   3. optimise + runnable  — a shape or fact tract cannot resolve
//!
//! Stage 2 is the one worth having: it names the operator rather than saying
//! the model did not load, which is the difference between "pick another
//! runtime" and "pick another export".
//!
//! ```sh
//! cargo run --release -- path/to/model.onnx
//! ```
//!
//! **What it does not do.** Run the model on anything. Scoring a candidate
//! against real pages belongs in the workspace, gated and reproducible —
//! `crates/taiyaku-cli/src/model_experiment.rs` is what that looks like, and it
//! is where the spike's own detector port went when it stopped being throwaway.
//! The overlays this directory used to draw are drawn there now, from the run
//! that produces the numbers rather than from a second decode.

use std::{collections::BTreeMap, env, path::PathBuf};

use tract_onnx::prelude::*;

fn main() {
    let Some(path) = env::args().nth(1).map(PathBuf::from) else {
        eprintln!("usage: probe <model.onnx>");
        std::process::exit(2);
    };

    // ---- stage 1: parse ---------------------------------------------------
    let proto = match tract_onnx::onnx().proto_model_for_path(&path) {
        Ok(p) => p,
        Err(e) => {
            println!("stage 1 parse: FAILED\n  {e:?}");
            std::process::exit(1);
        }
    };
    println!("stage 1 parse: ok");
    println!("  producer: {} {}", proto.producer_name, proto.producer_version);
    println!("  ir_version: {}", proto.ir_version);
    for o in &proto.opset_import {
        let domain = if o.domain.is_empty() { "ai.onnx" } else { &o.domain };
        println!("  opset: {domain} v{}", o.version);
    }
    if let Some(g) = proto.graph.as_ref() {
        println!("  graph: {} nodes, {} initializers", g.node.len(), g.initializer.len());
        let mut ops: BTreeMap<&str, usize> = BTreeMap::new();
        for n in &g.node {
            *ops.entry(n.op_type.as_str()).or_default() += 1;
        }
        println!("  operators in the file ({} distinct):", ops.len());
        for (op, n) in &ops {
            println!("    {op:<28} {n}");
        }
        for i in &g.input {
            println!("  file input:  {}", i.name);
        }
        for o in &g.output {
            println!("  file output: {}", o.name);
        }
    }

    // ---- stage 2: build the graph -----------------------------------------
    // This is where an operator tract has no rule for shows up, by name.
    let model = match tract_onnx::onnx().model_for_path(&path) {
        Ok(m) => m,
        Err(e) => {
            println!("stage 2 graph: FAILED");
            println!("  {e:?}");
            std::process::exit(1);
        }
    };
    println!("stage 2 graph: ok ({} nodes)", model.nodes().len());

    // ---- stage 3: optimise and make it runnable ---------------------------
    // The input is fixed here rather than left symbolic: comic-text-detector
    // is a letterboxed square-input YOLO, and tract cannot resolve output
    // facts without being told the batch and the side.
    let side = 1024;
    let with_shape = model.clone().with_input_fact(
        0,
        InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, side, side)),
    );
    let staged = match with_shape {
        Ok(m) => m,
        Err(e) => {
            println!("stage 3 shape: FAILED at 1x3x{side}x{side}\n  {e:?}");
            std::process::exit(1);
        }
    };
    match staged.into_optimized().and_then(|m| m.into_runnable()) {
        Ok(runnable) => {
            println!("stage 3 runnable: ok at 1x3x{side}x{side}");
            let m = runnable.model();
            for (i, o) in m.input_outlets().unwrap_or(&[]).iter().enumerate() {
                println!("  input  {i}: {:?}", m.outlet_fact(*o).map(|f| format!("{f:?}")));
            }
            for (i, o) in m.output_outlets().unwrap_or(&[]).iter().enumerate() {
                println!(
                    "  output {i}: {} {:?}",
                    m.node(o.node).name,
                    m.outlet_fact(*o).map(|f| format!("{f:?}"))
                );
            }
        }
        Err(e) => {
            println!("stage 3 runnable: FAILED at 1x3x{side}x{side}");
            println!("  {e:?}");
            std::process::exit(1);
        }
    }
}
