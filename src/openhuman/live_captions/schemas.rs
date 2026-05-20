//! Controller schemas for the `live_captions` domain.

use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use serde_json::{Map, Value};

type SchemaBuilder = fn() -> ControllerSchema;
type ControllerHandler = fn(Map<String, Value>) -> ControllerFuture;

struct Def {
    function: &'static str,
    schema: SchemaBuilder,
    handler: ControllerHandler,
}

const DEFS: &[Def] = &[
    Def {
        function: "start_transcript",
        schema: schema_start,
        handler: h_start,
    },
    Def {
        function: "append_segment",
        schema: schema_append,
        handler: h_append,
    },
    Def {
        function: "complete_transcript",
        schema: schema_complete,
        handler: h_complete,
    },
    Def {
        function: "summarize_transcript",
        schema: schema_summarize,
        handler: h_summarize,
    },
    Def {
        function: "get_transcript",
        schema: schema_get,
        handler: h_get,
    },
    Def {
        function: "list_transcripts",
        schema: schema_list,
        handler: h_list,
    },
    Def {
        function: "search_transcripts",
        schema: schema_search,
        handler: h_search,
    },
];

pub fn all_controller_schemas() -> Vec<ControllerSchema> {
    DEFS.iter().map(|d| (d.schema)()).collect()
}
pub fn all_registered_controllers() -> Vec<RegisteredController> {
    DEFS.iter()
        .map(|d| RegisteredController {
            schema: (d.schema)(),
            handler: d.handler,
        })
        .collect()
}
pub fn schemas(function: &str) -> ControllerSchema {
    DEFS.iter()
        .find(|d| d.function == function)
        .map(|d| (d.schema)())
        .unwrap_or_else(schema_unknown)
}

fn schema_start() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "start_transcript",
        description: "Start a new live caption transcript session.",
        inputs: vec![
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "Optional ID.",
                required: false,
            },
            FieldSchema {
                name: "source",
                ty: TypeSchema::String,
                comment: "microphone|system_audio|meet_call.",
                required: false,
            },
            FieldSchema {
                name: "title",
                ty: TypeSchema::String,
                comment: "Optional title.",
                required: false,
            },
        ],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "Transcript ID.",
                required: true,
            },
            FieldSchema {
                name: "state",
                ty: TypeSchema::String,
                comment: "State.",
                required: true,
            },
        ],
    }
}

fn schema_append() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "append_segment",
        description: "Append a caption segment to an active transcript.",
        inputs: vec![
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "Transcript ID.",
                required: true,
            },
            FieldSchema {
                name: "text",
                ty: TypeSchema::String,
                comment: "Segment text.",
                required: true,
            },
            FieldSchema {
                name: "start_ms",
                ty: TypeSchema::F64,
                comment: "Start time ms.",
                required: true,
            },
            FieldSchema {
                name: "end_ms",
                ty: TypeSchema::F64,
                comment: "End time ms.",
                required: true,
            },
            FieldSchema {
                name: "speaker",
                ty: TypeSchema::String,
                comment: "Speaker label.",
                required: false,
            },
            FieldSchema {
                name: "confidence",
                ty: TypeSchema::F64,
                comment: "STT confidence.",
                required: false,
            },
            FieldSchema {
                name: "is_final",
                ty: TypeSchema::Bool,
                comment: "Final segment flag.",
                required: false,
            },
        ],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "segment_count",
                ty: TypeSchema::F64,
                comment: "Total segments.",
                required: true,
            },
        ],
    }
}

fn schema_complete() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "complete_transcript",
        description: "Mark a transcript as completed.",
        inputs: vec![FieldSchema {
            name: "transcript_id",
            ty: TypeSchema::String,
            comment: "Transcript ID.",
            required: true,
        }],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "ID.",
                required: true,
            },
            FieldSchema {
                name: "state",
                ty: TypeSchema::String,
                comment: "State.",
                required: true,
            },
            FieldSchema {
                name: "segments",
                ty: TypeSchema::F64,
                comment: "Segment count.",
                required: true,
            },
        ],
    }
}

fn schema_summarize() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "summarize_transcript",
        description: "Generate a summary for a completed transcript.",
        inputs: vec![FieldSchema {
            name: "transcript_id",
            ty: TypeSchema::String,
            comment: "Transcript ID.",
            required: true,
        }],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "ID.",
                required: true,
            },
            FieldSchema {
                name: "summary",
                ty: TypeSchema::String,
                comment: "Generated summary.",
                required: true,
            },
        ],
    }
}

fn schema_get() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "get_transcript",
        description: "Get transcript details.",
        inputs: vec![FieldSchema {
            name: "transcript_id",
            ty: TypeSchema::String,
            comment: "Transcript ID.",
            required: true,
        }],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "transcript_id",
                ty: TypeSchema::String,
                comment: "ID.",
                required: true,
            },
            FieldSchema {
                name: "state",
                ty: TypeSchema::String,
                comment: "State.",
                required: true,
            },
            FieldSchema {
                name: "segments",
                ty: TypeSchema::F64,
                comment: "Segment count.",
                required: true,
            },
            FieldSchema {
                name: "duration_ms",
                ty: TypeSchema::F64,
                comment: "Duration.",
                required: true,
            },
        ],
    }
}

fn schema_list() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "list_transcripts",
        description: "List all transcripts.",
        inputs: vec![],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "transcripts",
                ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                comment: "Transcript list.",
                required: true,
            },
        ],
    }
}

fn schema_unknown() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "unknown",
        description: "Unknown live_captions function.",
        inputs: vec![FieldSchema {
            name: "function",
            ty: TypeSchema::String,
            comment: "Requested.",
            required: true,
        }],
        outputs: vec![FieldSchema {
            name: "error",
            ty: TypeSchema::String,
            comment: "Error.",
            required: true,
        }],
    }
}

fn h_start(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_start_transcript(p).await })
}
fn h_append(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_append_segment(p).await })
}
fn h_complete(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_complete_transcript(p).await })
}
fn h_summarize(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_summarize_transcript(p).await })
}
fn h_get(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_get_transcript(p).await })
}
fn h_list(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_list_transcripts(p).await })
}
fn h_search(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_search_transcripts(p).await })
}

fn schema_search() -> ControllerSchema {
    ControllerSchema {
        namespace: "live_captions",
        function: "search_transcripts",
        description: "Search transcripts by text content.",
        inputs: vec![FieldSchema {
            name: "query",
            ty: TypeSchema::String,
            comment: "Search query.",
            required: true,
        }],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "results",
                ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                comment: "Matching transcripts.",
                required: true,
            },
            FieldSchema {
                name: "count",
                ty: TypeSchema::F64,
                comment: "Result count.",
                required: true,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handlers_match_schemas() {
        let s: Vec<_> = all_controller_schemas()
            .into_iter()
            .map(|s| s.function)
            .collect();
        let h: Vec<_> = all_registered_controllers()
            .into_iter()
            .map(|c| c.schema.function)
            .collect();
        assert_eq!(s, h);
        assert_eq!(s.len(), 7);
    }

    #[test]
    fn all_have_namespace() {
        for s in all_controller_schemas() {
            assert_eq!(s.namespace, "live_captions");
        }
    }

    #[test]
    fn unknown_lookup() {
        assert_eq!(schemas("nope").function, "unknown");
    }
}
