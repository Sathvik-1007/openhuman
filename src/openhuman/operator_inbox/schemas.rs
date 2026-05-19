//! Controller schemas for `operator_inbox` domain.
use crate::core::all::{ControllerFuture, RegisteredController};
use crate::core::{ControllerSchema, FieldSchema, TypeSchema};
use serde_json::{Map, Value};

type SB = fn() -> ControllerSchema;
type CH = fn(Map<String, Value>) -> ControllerFuture;
struct Def {
    function: &'static str,
    schema: SB,
    handler: CH,
}

const DEFS: &[Def] = &[
    Def {
        function: "triage_message",
        schema: s_triage,
        handler: h_triage,
    },
    Def {
        function: "generate_draft",
        schema: s_draft,
        handler: h_draft,
    },
    Def {
        function: "schedule_followup",
        schema: s_followup,
        handler: h_followup,
    },
    Def {
        function: "get_triage",
        schema: s_get,
        handler: h_get,
    },
    Def {
        function: "list_triage",
        schema: s_list,
        handler: h_list,
    },
    Def {
        function: "archive",
        schema: s_archive,
        handler: h_archive,
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
        .unwrap_or_else(s_unknown)
}

fn s_triage() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "triage_message",
        description: "Triage an incoming message and score priority.",
        inputs: vec![
            FieldSchema {
                name: "source",
                ty: TypeSchema::String,
                comment: "email|chat|social|webhook.",
                required: false,
            },
            FieldSchema {
                name: "sender",
                ty: TypeSchema::String,
                comment: "Sender.",
                required: true,
            },
            FieldSchema {
                name: "subject",
                ty: TypeSchema::String,
                comment: "Subject.",
                required: true,
            },
            FieldSchema {
                name: "body",
                ty: TypeSchema::String,
                comment: "Body.",
                required: true,
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
                name: "triage_id",
                ty: TypeSchema::String,
                comment: "ID.",
                required: true,
            },
            FieldSchema {
                name: "priority",
                ty: TypeSchema::String,
                comment: "Priority.",
                required: true,
            },
        ],
    }
}

fn s_draft() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "generate_draft",
        description: "Generate a reply draft.",
        inputs: vec![
            FieldSchema {
                name: "triage_id",
                ty: TypeSchema::String,
                comment: "Triage ID.",
                required: true,
            },
            FieldSchema {
                name: "tone",
                ty: TypeSchema::String,
                comment: "professional|casual|formal.",
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
                name: "content",
                ty: TypeSchema::String,
                comment: "Draft content.",
                required: true,
            },
        ],
    }
}

fn s_followup() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "schedule_followup",
        description: "Schedule a follow-up.",
        inputs: vec![
            FieldSchema {
                name: "triage_id",
                ty: TypeSchema::String,
                comment: "Triage ID.",
                required: true,
            },
            FieldSchema {
                name: "follow_up_at",
                ty: TypeSchema::U64,
                comment: "Unix timestamp.",
                required: true,
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
                name: "follow_up_at",
                ty: TypeSchema::U64,
                comment: "Scheduled time.",
                required: true,
            },
        ],
    }
}

fn s_get() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "get_triage",
        description: "Get triage record.",
        inputs: vec![FieldSchema {
            name: "triage_id",
            ty: TypeSchema::String,
            comment: "ID.",
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
                name: "triage_id",
                ty: TypeSchema::String,
                comment: "ID.",
                required: true,
            },
            FieldSchema {
                name: "priority",
                ty: TypeSchema::String,
                comment: "Priority.",
                required: true,
            },
        ],
    }
}

fn s_list() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "list_triage",
        description: "List all triage records.",
        inputs: vec![],
        outputs: vec![
            FieldSchema {
                name: "ok",
                ty: TypeSchema::Bool,
                comment: "Success.",
                required: true,
            },
            FieldSchema {
                name: "records",
                ty: TypeSchema::Array(Box::new(TypeSchema::Json)),
                comment: "Records.",
                required: true,
            },
        ],
    }
}

fn s_archive() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "archive",
        description: "Archive a triage record.",
        inputs: vec![FieldSchema {
            name: "triage_id",
            ty: TypeSchema::String,
            comment: "ID.",
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
                name: "status",
                ty: TypeSchema::String,
                comment: "New status.",
                required: true,
            },
        ],
    }
}

fn s_unknown() -> ControllerSchema {
    ControllerSchema {
        namespace: "operator_inbox",
        function: "unknown",
        description: "Unknown.",
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

fn h_triage(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_triage_message(p).await })
}
fn h_draft(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_generate_draft(p).await })
}
fn h_followup(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_schedule_followup(p).await })
}
fn h_get(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_get_triage(p).await })
}
fn h_list(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_list_triage(p).await })
}
fn h_archive(p: Map<String, Value>) -> ControllerFuture {
    Box::pin(async move { super::rpc::handle_archive(p).await })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handlers_match() {
        assert_eq!(
            all_controller_schemas().len(),
            all_registered_controllers().len()
        );
        assert_eq!(all_controller_schemas().len(), 6);
    }
    #[test]
    fn namespace() {
        for s in all_controller_schemas() {
            assert_eq!(s.namespace, "operator_inbox");
        }
    }
    #[test]
    fn unknown() {
        assert_eq!(schemas("nope").function, "unknown");
    }
}
