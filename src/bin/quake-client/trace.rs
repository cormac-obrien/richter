use std::{cell::RefCell, fs::File, io::BufWriter, rc::Rc};

use richter::{client::trace::TraceFrame, common::console::CvarRegistry};

const DEFAULT_TRACE_PATH: &'static str = "richter-trace.json";

/// Implements the `trace_begin` command.
pub fn cmd_trace_begin(
    trace: Rc<RefCell<Option<Vec<TraceFrame>>>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        if trace.borrow().is_some() {
            log::error!("trace already in progress");
            "trace already in progress".to_owned()
        } else {
            // start a new trace
            trace.replace(Some(Vec::new()));
            String::new()
        }
    })
}

/// Implements the `trace_end` command.
pub fn cmd_trace_end(
    cvars: Rc<RefCell<CvarRegistry>>,
    trace: Rc<RefCell<Option<Vec<TraceFrame>>>>,
) -> Box<dyn Fn(&[&str]) -> String> {
    Box::new(move |_| {
        if let Some(trace_frames) = trace.replace(None) {
            let trace_path = cvars
                .borrow()
                .get("trace_path")
                .unwrap_or(DEFAULT_TRACE_PATH.to_string());
            let trace_file = match File::create(&trace_path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Couldn't open trace file for write: {}", e);
                    return format!("Couldn't open trace file for write: {}", e);
                }
            };

            let mut writer = BufWriter::new(trace_file);

            match serde_json::to_writer(&mut writer, &trace_frames) {
                Ok(()) => (),
                Err(e) => {
                    log::error!("Couldn't serialize trace: {}", e);
                    return format!("Couldn't serialize trace: {}", e);
                }
            };

            log::debug!("wrote {} frames to {}", trace_frames.len(), &trace_path);
            format!("wrote {} frames to {}", trace_frames.len(), &trace_path)
        } else {
            log::error!("no trace in progress");
            "no trace in progress".to_owned()
        }
    })
}
